# Streamline

[![CI](https://github.com/Laurits7/Streamline/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Laurits7/Streamline/actions/workflows/ci.yml)
[![Docs](https://img.shields.io/badge/docs-laurits7.github.io%2FStreamline-blue)](https://laurits7.github.io/Streamline/)
[![Release](https://img.shields.io/github/v/release/Laurits7/Streamline)](https://github.com/Laurits7/Streamline/releases/latest)
[![License: GPL-3.0-or-later](https://img.shields.io/github/license/Laurits7/Streamline)](LICENSE)
[![Image](https://img.shields.io/badge/image-ghcr.io%2Flaurits7%2Fstreamline-2496ED?logo=docker&logoColor=white)](https://github.com/Laurits7/Streamline/pkgs/container/streamline)
[![Desktop app](https://img.shields.io/badge/desktop-Linux%20%28.deb%2C%20tarball%29-FCC624?logo=linux&logoColor=black)](https://laurits7.github.io/Streamline/getting-started/desktop/)

[![rustfmt](https://img.shields.io/badge/rustfmt-checked-success?logo=rust)](.github/workflows/ci.yml)
[![clippy](https://img.shields.io/badge/clippy-no%20warnings-success?logo=rust)](.github/workflows/ci.yml)
[![TypeScript](https://img.shields.io/badge/TypeScript-strict-success?logo=typescript&logoColor=white)](web/tsconfig.app.json)
[![Bundle](https://img.shields.io/badge/initial%20load-%3C150%20KB%20gzip-success)](.github/workflows/ci.yml)

A self-hosted todo list and day planner for a household. One small container, one data
folder, and it works from any phone, tablet or computer on your network, plus a Linux desktop
app that also works offline.

**📖 Documentation: <https://laurits7.github.io/Streamline/>**

- **Plan your day**: a Today view with a timeline, a planning ritual, a week view and time
  blocks around your calendar.
- **Share the household**: groups share projects, chores and routines live; day plans and
  journals stay personal. Projects can even be shared with people on other Streamline servers.
- **Routines and chores**: any schedule, multi-step chores, checklists, prerequisites, waiting
  for results and ongoing tasks.
- **Reflect**: journal and tracking, long-term goals, habit goals and a weekly review.
- **Connect**: CalDAV calendars (read and write-back) and tasks from GitHub, GitLab and Gitea.
- **Small**: a 16 MB image using about 9 MB of memory; one SQLite file on your own machine.

The full list is on the [Features](https://laurits7.github.io/Streamline/features/) page.

## Quick start

Each [release](https://github.com/Laurits7/Streamline/releases) has a `docker-compose.yml`
using the prebuilt image (amd64 and arm64). Put it in an empty directory and run:

```sh
docker compose up -d
```

Then open `http://<server-ip>:3000` and create the admin account. Your data is in `./data`.

Next steps: [configuration](https://laurits7.github.io/Streamline/getting-started/configuration/),
[HTTPS for the phone app and notifications](https://laurits7.github.io/Streamline/getting-started/https/),
[backups and updates](https://laurits7.github.io/Streamline/getting-started/backups-and-updates/)
and the [Linux desktop app](https://laurits7.github.io/Streamline/getting-started/desktop/).

## Development

Rust 1.88+ and Node 22+. `make dev-server` and `make dev-web` run it with hot reload;
`make test` and `make check` run the tests and linters. See
[Developing](https://laurits7.github.io/Streamline/development/) for the layout and the
[spec](docs/SPEC.md), [work plan](docs/WORKPLAN.md) and [decisions](docs/DECISIONS.md) behind it.
What changed in each version is in the [changelog](CHANGELOG.md).

## License

[GPL-3.0-or-later](LICENSE)
