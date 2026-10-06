//! A small CalDAV client (RFC 4791), hand-rolled on `reqwest` + `quick-xml` (D-55):
//! discovery, listing calendars, listing object etags and fetching objects. Read-only for
//! now; `CalendarProvider` reserves the write calls for later write-back (SPEC §6.5).

use std::time::Duration;

use anyhow::{Context, anyhow, bail};
use quick_xml::{escape::resolve_predefined_entity, events::Event, reader::Reader};
use reqwest::{Method, StatusCode, Url};

/// A calendar collection on the server.
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteCalendar {
    /// Absolute URL of the collection.
    pub href: String,
    pub name: String,
    pub color: Option<String>,
    /// Changes whenever anything in the calendar changes (CalendarServer `getctag`, or the
    /// RFC 6578 sync token when there is no ctag).
    pub ctag: Option<String>,
}

/// One calendar resource (usually one event with its exceptions).
#[derive(Debug, Clone, PartialEq)]
pub struct RemoteObject {
    pub href: String,
    pub etag: String,
    pub ics: String,
}

/// A source of calendar events. CalDAV is the only one in v1; writing is reserved for
/// calendar write-back (WORKPLAN 7.7).
#[allow(async_fn_in_trait)]
pub trait CalendarProvider {
    async fn list_calendars(&self) -> anyhow::Result<Vec<RemoteCalendar>>;
    /// `(href, etag)` of every event resource in the calendar.
    async fn list_objects(&self, calendar_href: &str) -> anyhow::Result<Vec<(String, String)>>;
    async fn fetch_objects(
        &self,
        calendar_href: &str,
        hrefs: &[String],
    ) -> anyhow::Result<Vec<RemoteObject>>;
    async fn put_event(&self, _calendar_href: &str, _ics: &str) -> anyhow::Result<()> {
        bail!("writing to the calendar is not supported yet")
    }
    async fn delete_event(&self, _href: &str) -> anyhow::Result<()> {
        bail!("writing to the calendar is not supported yet")
    }
}

pub struct CalDav {
    http: reqwest::Client,
    base: Url,
    username: String,
    password: String,
}

const NS: &str = r#"xmlns:d="DAV:" xmlns:c="urn:ietf:params:xml:ns:caldav" xmlns:cs="http://calendarserver.org/ns/" xmlns:i="http://apple.com/ns/ical/""#;

/// Splits `user:password@` out of a URL. Returns the clean URL and the credentials found.
pub fn split_credentials(url: &str) -> anyhow::Result<(String, Option<(String, String)>)> {
    let mut u = Url::parse(url.trim()).context("not a valid URL")?;
    if !matches!(u.scheme(), "http" | "https") {
        bail!("the URL must start with http:// or https://");
    }
    let creds = (!u.username().is_empty()).then(|| {
        (
            percent_decode(u.username()),
            percent_decode(u.password().unwrap_or("")),
        )
    });
    let _ = u.set_username("");
    let _ = u.set_password(None);
    Ok((u.to_string(), creds))
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%'
            && i + 2 < b.len()
            && let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(v);
            i += 3;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn install_crypto() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
    });
}

impl CalDav {
    pub fn new(url: &str, username: &str, password: &str) -> anyhow::Result<Self> {
        install_crypto();
        let mut base = Url::parse(url).context("not a valid URL")?;
        if !base.path().ends_with('/') {
            base.set_path(&format!("{}/", base.path()));
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent(concat!("Streamline/", env!("CARGO_PKG_VERSION")))
            .build()?;
        Ok(Self {
            http,
            base,
            username: username.into(),
            password: password.into(),
        })
    }

    fn resolve(&self, href: &str) -> anyhow::Result<Url> {
        self.base
            .join(href)
            .with_context(|| format!("bad href {href:?}"))
    }

    async fn dav(
        &self,
        method: &str,
        url: &Url,
        depth: &str,
        body: String,
    ) -> anyhow::Result<Node> {
        let res = self
            .http
            .request(Method::from_bytes(method.as_bytes())?, url.clone())
            .basic_auth(&self.username, Some(&self.password))
            .header("Depth", depth)
            .header("Content-Type", "application/xml; charset=utf-8")
            .body(body)
            .send()
            .await
            .map_err(|e| {
                anyhow::Error::new(e.without_url()).context("can't reach the calendar server")
            })?;
        match res.status() {
            StatusCode::MULTI_STATUS | StatusCode::OK => {}
            StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
                bail!("the calendar server rejected the username or password")
            }
            StatusCode::NOT_FOUND => bail!("nothing found at {} (404)", url.path()),
            s => bail!(
                "the calendar server answered {s} for {method} {}",
                url.path()
            ),
        }
        let text = res.text().await?;
        parse_xml(&text).context("the calendar server sent XML we can't read")
    }

    async fn propfind(
        &self,
        url: &Url,
        depth: &str,
        props: &str,
    ) -> anyhow::Result<Vec<DavResponse>> {
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><d:propfind {NS}><d:prop>{props}</d:prop></d:propfind>"#
        );
        Ok(responses(&self.dav("PROPFIND", url, depth, body).await?))
    }

    /// `href` of a property that holds one (`current-user-principal`, `calendar-home-set`).
    async fn href_prop(&self, url: &Url, prop: &str, name: &str) -> anyhow::Result<Option<Url>> {
        let rs = self.propfind(url, "0", prop).await?;
        let found = rs.iter().find_map(|r| {
            r.prop(name)?
                .find("href")
                .map(|h| h.text.trim().to_string())
        });
        found
            .filter(|h| !h.is_empty())
            .map(|h| self.resolve(&h))
            .transpose()
    }
}

const CAL_PROPS: &str = "<d:resourcetype/><d:displayname/><i:calendar-color/><cs:getctag/><d:sync-token/><c:supported-calendar-component-set/>";

fn calendar_of(r: &DavResponse, base: &Url) -> Option<RemoteCalendar> {
    let rt = r.prop("resourcetype")?;
    rt.find("calendar")?;
    // Skip collections that can't hold events (e.g. task lists).
    if let Some(set) = r.prop("supported-calendar-component-set") {
        let comps: Vec<&str> = set.all("comp").filter_map(|c| c.attr("name")).collect();
        if !comps.is_empty() && !comps.iter().any(|c| c.eq_ignore_ascii_case("VEVENT")) {
            return None;
        }
    }
    let href = base.join(&r.href).ok()?.to_string();
    let text = |n: &str| {
        r.prop(n)
            .map(|p| p.text.trim().to_string())
            .filter(|t| !t.is_empty())
    };
    let name = text("displayname").unwrap_or_else(|| {
        href.trim_end_matches('/')
            .rsplit('/')
            .next()
            .unwrap_or("Calendar")
            .to_string()
    });
    // Apple colours can be #RRGGBBAA; keep #RRGGBB.
    let color = text("calendar-color").map(|c| {
        if c.len() == 9 && c.starts_with('#') {
            c[..7].to_string()
        } else {
            c
        }
    });
    Some(RemoteCalendar {
        href,
        name,
        color,
        ctag: text("getctag").or_else(|| text("sync-token")),
    })
}

impl CalendarProvider for CalDav {
    async fn list_calendars(&self) -> anyhow::Result<Vec<RemoteCalendar>> {
        // The URL may point straight at a calendar.
        let here = self.propfind(&self.base, "0", CAL_PROPS).await?;
        if let Some(c) = here.iter().find_map(|r| calendar_of(r, &self.base)) {
            return Ok(vec![c]);
        }
        let principal = self
            .href_prop(
                &self.base,
                "<d:current-user-principal/>",
                "current-user-principal",
            )
            .await?
            .unwrap_or_else(|| self.base.clone());
        let home = self
            .href_prop(&principal, "<c:calendar-home-set/>", "calendar-home-set")
            .await?
            .unwrap_or(principal);
        let mut cals: Vec<RemoteCalendar> = self
            .propfind(&home, "1", CAL_PROPS)
            .await?
            .iter()
            .filter_map(|r| calendar_of(r, &home))
            .collect();
        cals.sort_by_key(|a| a.name.to_lowercase());
        Ok(cals)
    }

    async fn list_objects(&self, calendar_href: &str) -> anyhow::Result<Vec<(String, String)>> {
        let url = self.resolve(calendar_href)?;
        let body = format!(
            r#"<?xml version="1.0" encoding="utf-8"?><c:calendar-query {NS}><d:prop><d:getetag/></d:prop><c:filter><c:comp-filter name="VCALENDAR"><c:comp-filter name="VEVENT"/></c:comp-filter></c:filter></c:calendar-query>"#
        );
        let rs = responses(&self.dav("REPORT", &url, "1", body).await?);
        Ok(rs
            .iter()
            .filter(|r| !r.href.ends_with('/'))
            .filter_map(|r| {
                let etag = r.prop("getetag")?.text.trim().to_string();
                Some((url.join(&r.href).ok()?.to_string(), etag))
            })
            .collect())
    }

    async fn fetch_objects(
        &self,
        calendar_href: &str,
        hrefs: &[String],
    ) -> anyhow::Result<Vec<RemoteObject>> {
        let url = self.resolve(calendar_href)?;
        let mut out = vec![];
        for chunk in hrefs.chunks(50) {
            let list: String = chunk
                .iter()
                .filter_map(|h| Url::parse(h).ok())
                .map(|h| format!("<d:href>{}</d:href>", xml_escape(h.path())))
                .collect();
            let body = format!(
                r#"<?xml version="1.0" encoding="utf-8"?><c:calendar-multiget {NS}><d:prop><d:getetag/><c:calendar-data/></d:prop>{list}</c:calendar-multiget>"#
            );
            for r in responses(&self.dav("REPORT", &url, "1", body).await?) {
                let (Some(etag), Some(data)) = (r.prop("getetag"), r.prop("calendar-data")) else {
                    continue;
                };
                out.push(RemoteObject {
                    href: url.join(&r.href)?.to_string(),
                    etag: etag.text.trim().to_string(),
                    ics: data.text.clone(),
                });
            }
        }
        Ok(out)
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

// ----- A minimal XML tree (namespaces dropped: DAV property names don't clash here) -----

#[derive(Debug, Default, Clone)]
pub struct Node {
    pub name: String,
    pub attrs: Vec<(String, String)>,
    pub text: String,
    pub children: Vec<Node>,
}

impl Node {
    pub fn child(&self, name: &str) -> Option<&Node> {
        self.children.iter().find(|c| c.name == name)
    }
    pub fn children_named<'a>(&'a self, name: &'a str) -> impl Iterator<Item = &'a Node> {
        self.children.iter().filter(move |c| c.name == name)
    }
    /// First descendant (or self) with this name.
    pub fn find(&self, name: &str) -> Option<&Node> {
        if self.name == name {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(name))
    }
    /// All descendants with this name.
    pub fn all<'a>(&'a self, name: &'a str) -> Box<dyn Iterator<Item = &'a Node> + 'a> {
        Box::new(self.children.iter().flat_map(move |c| {
            std::iter::once(c)
                .filter(move |c| c.name == name)
                .chain(c.all(name))
        }))
    }
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

fn local(name: &str) -> String {
    name.rsplit(':').next().unwrap_or("").to_string()
}

pub fn parse_xml(text: &str) -> anyhow::Result<Node> {
    let mut reader = Reader::from_str(text);
    let mut stack: Vec<Node> = vec![Node::default()];
    let open = |e: &quick_xml::events::BytesStart| Node {
        name: local(e.name().as_ref()),
        attrs: e
            .attributes()
            .flatten()
            .map(|a| {
                (
                    local(a.key.as_ref()),
                    a.normalized_value(quick_xml::XmlVersion::Explicit1_0)
                        .map(|v| v.into_owned())
                        .unwrap_or_default(),
                )
            })
            .collect(),
        ..Default::default()
    };
    loop {
        match reader.read_event()? {
            Event::Start(e) => stack.push(open(&e)),
            Event::Empty(e) => {
                let n = open(&e);
                stack.last_mut().unwrap().children.push(n);
            }
            Event::End(_) => {
                let n = stack.pop().ok_or_else(|| anyhow!("unbalanced XML"))?;
                stack
                    .last_mut()
                    .ok_or_else(|| anyhow!("unbalanced XML"))?
                    .children
                    .push(n);
            }
            Event::Text(t) => stack.last_mut().unwrap().text.push_str(&t.xml10_content()),
            Event::CData(t) => stack.last_mut().unwrap().text.push_str(&t),
            Event::GeneralRef(r) => {
                let s = match r.resolve_char_ref()? {
                    Some(c) => c.to_string(),
                    None => resolve_predefined_entity(&r).unwrap_or("").to_string(),
                };
                stack.last_mut().unwrap().text.push_str(&s);
            }
            Event::Eof => break,
            _ => {}
        }
    }
    let mut root = stack.pop().ok_or_else(|| anyhow!("empty XML"))?;
    if !stack.is_empty() {
        bail!("unbalanced XML");
    }
    root.children.pop().ok_or_else(|| anyhow!("empty XML"))
}

/// One `<response>` of a multistatus: its href and the properties found (status 200).
#[derive(Debug)]
pub struct DavResponse {
    pub href: String,
    pub props: Vec<Node>,
}

impl DavResponse {
    pub fn prop(&self, name: &str) -> Option<&Node> {
        self.props.iter().find(|p| p.name == name)
    }
}

pub fn responses(root: &Node) -> Vec<DavResponse> {
    root.children_named("response")
        .filter_map(|r| {
            let href = r.child("href")?.text.trim().to_string();
            let props = r
                .children_named("propstat")
                .filter(|ps| ps.child("status").is_none_or(|s| s.text.contains(" 200")))
                .filter_map(|ps| ps.child("prop"))
                .flat_map(|p| p.children.iter().cloned())
                .collect();
            Some(DavResponse { href, props })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // Recorded from Radicale 3.8 (D-55).
    const HOME: &str = r##"<?xml version='1.0' encoding='utf-8'?>
<multistatus xmlns="DAV:" xmlns:C="urn:ietf:params:xml:ns:caldav" xmlns:CS="http://calendarserver.org/ns/" xmlns:ICAL="http://apple.com/ns/ical/"><response><href>/anna/</href><propstat><prop><resourcetype><principal /><collection /></resourcetype></prop><status>HTTP/1.1 200 OK</status></propstat><propstat><prop><displayname /><ICAL:calendar-color /><CS:getctag /></prop><status>HTTP/1.1 404 Not Found</status></propstat></response><response><href>/anna/home/</href><propstat><prop><resourcetype><C:calendar /><collection /></resourcetype><displayname>Home &amp; garden</displayname><ICAL:calendar-color>#3b82f6ff</ICAL:calendar-color><CS:getctag>"2218"</CS:getctag><C:supported-calendar-component-set><C:comp name="VTODO" /><C:comp name="VEVENT" /></C:supported-calendar-component-set></prop><status>HTTP/1.1 200 OK</status></propstat></response><response><href>/anna/tasks/</href><propstat><prop><resourcetype><C:calendar /><collection /></resourcetype><displayname>Tasks</displayname><C:supported-calendar-component-set><C:comp name="VTODO" /></C:supported-calendar-component-set></prop><status>HTTP/1.1 200 OK</status></propstat></response></multistatus>"##;

    #[test]
    fn reads_calendar_collections() {
        let base = Url::parse("http://radicale:5232/anna/").unwrap();
        let rs = responses(&parse_xml(HOME).unwrap());
        assert_eq!(rs.len(), 3);
        assert!(
            rs[0].prop("displayname").is_none(),
            "404 propstats are ignored"
        );
        let cals: Vec<_> = rs.iter().filter_map(|r| calendar_of(r, &base)).collect();
        assert_eq!(
            cals,
            [RemoteCalendar {
                href: "http://radicale:5232/anna/home/".into(),
                name: "Home & garden".into(),
                color: Some("#3b82f6".into()),
                ctag: Some("\"2218\"".into()),
            }],
            "the task-only list is skipped"
        );
    }

    #[test]
    fn calendar_data_keeps_its_text() {
        let xml = "<d:multistatus xmlns:d=\"DAV:\" xmlns:c=\"urn:ietf:params:xml:ns:caldav\"><d:response><d:href>/a/b/x.ics</d:href><d:propstat><d:prop><d:getetag>\"e1\"</d:getetag><c:calendar-data><![CDATA[BEGIN:VCALENDAR\r\nSUMMARY:A <b>\r\nEND:VCALENDAR\r\n]]></c:calendar-data></d:prop><d:status>HTTP/1.1 200 OK</d:status></d:propstat></d:response></d:multistatus>";
        let rs = responses(&parse_xml(xml).unwrap());
        assert_eq!(
            rs[0].prop("calendar-data").unwrap().text,
            "BEGIN:VCALENDAR\r\nSUMMARY:A <b>\r\nEND:VCALENDAR\r\n"
        );
        let xml2 = "<multistatus xmlns=\"DAV:\"><response><href>/a/b/x.ics</href><propstat><prop><calendar-data>SUMMARY:Tom &amp; Jerry&#10;</calendar-data></prop></propstat></response></multistatus>";
        assert_eq!(
            responses(&parse_xml(xml2).unwrap())[0]
                .prop("calendar-data")
                .unwrap()
                .text,
            "SUMMARY:Tom & Jerry\n"
        );
    }

    #[test]
    fn credentials_in_the_url_are_split_off() {
        let (url, creds) = split_credentials("https://anna:p%40ss@cal.example.org/dav").unwrap();
        assert_eq!(url, "https://cal.example.org/dav");
        assert_eq!(creds, Some(("anna".into(), "p@ss".into())));
        assert!(split_credentials("ftp://x").is_err());
    }
}
