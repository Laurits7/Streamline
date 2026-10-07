#!/usr/bin/env python3
"""Draw the Streamline logo (D-73): a stone in a stream, with streamlines of potential flow
past a cylinder, psi = y(1 - R^2/r^2). Writes the SVG icons and renders the PNGs.

Run from the repo root: python3 scripts/logo.py  (needs rsvg-convert)
"""
import math
import subprocess
# Potential flow past a cylinder: psi = y(1 - R^2/r^2). For each streamline value c, solve y(x).
def streamline(c, R, x0, x1, n=60):
    pts=[]
    for i in range(n+1):
        x=x0+(x1-x0)*i/n
        lo,hi=(R*0.999 if c>0 else -40),(40 if c>0 else -R*0.999)
        # psi monotonic in |y| outside cylinder for fixed x; bisection on y with sign of c
        a,b=(0.0001,40.0)
        f=lambda y: y*(1-R*R/(x*x+y*y))-abs(c)
        # find y>=sqrt(max(R^2-x^2,0)) where f=0
        a=math.sqrt(max(R*R-x*x,0))+1e-6
        for _ in range(80):
            m=(a+b)/2
            if f(m)>0: b=m
            else: a=m
        y=(a+b)/2
        pts.append((x, y if c>0 else -y))
    return pts
def path(pts,cx,cy):
    # smooth: Catmull-Rom to cubic
    P=[(cx+x,cy+y) for x,y in pts]
    d=f"M{P[0][0]:.1f} {P[0][1]:.1f}"
    for i in range(len(P)-1):
        p0=P[max(i-1,0)];p1=P[i];p2=P[i+1];p3=P[min(i+2,len(P)-1)]
        c1=(p1[0]+(p2[0]-p0[0])/6,p1[1]+(p2[1]-p0[1])/6)
        c2=(p2[0]-(p3[0]-p1[0])/6,p2[1]-(p3[1]-p1[1])/6)
        d+=f"C{c1[0]:.1f} {c1[1]:.1f} {c2[0]:.1f} {c2[1]:.1f} {p2[0]:.1f} {p2[1]:.1f}"
    return d
def svg(bg, R, cs, widths, ops, rock, lines="#e3f0ff", cx=32, cy=32, n=24):
    out=[f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 64 64"><defs><linearGradient id="w" x1="0" y1="0" x2="0" y2="1"><stop offset="0" stop-color="{bg[0]}"/><stop offset="1" stop-color="{bg[1]}"/></linearGradient><linearGradient id="s" x1=".2" y1="0" x2=".8" y2="1"><stop offset="0" stop-color="#c9cdd4"/><stop offset="1" stop-color="#7b818c"/></linearGradient><clipPath id="c"><rect width="64" height="64" rx="14"/></clipPath></defs>',
         '<rect width="64" height="64" rx="14" fill="url(#w)"/><g clip-path="url(#c)" fill="none" stroke="'+lines+'" stroke-linecap="round">']
    for c,w,o in zip(cs,widths,ops):
        for s in (1,-1):
            out.append(f'<path d="{path(streamline(s*c,R,-cx-2,64-cx+2,n),cx,cy)}" stroke-width="{w}" opacity="{o}"/>')
    out.append('</g>'+rock+'</svg>')
    return ''.join(out)
# irregular stone, radius ~ R-2.5, centred (32,32)
def stone(cx,cy,r):
    k=[1.0,0.93,1.04,0.97,0.9,1.02,0.95,1.06,0.98,0.92]
    P=[]
    for i,f in enumerate(k):
        a=2*math.pi*i/len(k)-0.3
        P.append((cx+r*f*math.cos(a)*1.1, cy+r*f*math.sin(a)*0.92))
    d=f"M{(P[0][0]+P[1][0])/2:.1f} {(P[0][1]+P[1][1])/2:.1f}"
    for i in range(len(P)):
        p=P[(i+1)%len(P)]; q=P[(i+2)%len(P)]
        d+=f"Q{p[0]:.1f} {p[1]:.1f} {(p[0]+q[0])/2:.1f} {(p[1]+q[1])/2:.1f}"
    hl=f'<path d="M{cx-r*0.55:.1f} {cy-r*0.25:.1f}Q{cx-r*0.3:.1f} {cy-r*0.72:.1f} {cx+r*0.2:.1f} {cy-r*0.68:.1f}" fill="none" stroke="#eef0f3" stroke-width="1.8" stroke-linecap="round" opacity=".8"/>'
    return f'<path d="{d}Z" fill="url(#s)"/>'+hl


SVG = svg(("#2f86f0", "#1a5bc0"), 12, [3, 9, 16, 24], [3, 2.8, 2.6, 2.4], [0.95, 0.75, 0.55, 0.35], stone(32, 32, 9.3), n=14)
FULL_BLEED = SVG.replace('rx="14"', 'rx="0"')  # maskable and Apple icons get their own mask

if __name__ == "__main__":
    out = "web/public"
    for path in (f"{out}/favicon.svg", f"{out}/icons/icon.svg"):
        open(path, "w").write(SVG)
    open("/tmp/streamline-full-bleed.svg", "w").write(FULL_BLEED)
    for src, size, name in [
        (f"{out}/icons/icon.svg", 192, "icon-192.png"),
        (f"{out}/icons/icon.svg", 512, "icon-512.png"),
        ("/tmp/streamline-full-bleed.svg", 512, "maskable-512.png"),
        ("/tmp/streamline-full-bleed.svg", 180, "apple-touch-icon.png"),
    ]:
        subprocess.run(["rsvg-convert", "-w", str(size), src, "-o", f"{out}/icons/{name}"], check=True)
    print("Done. Bump SHELL in web/public/sw.js so installed apps pick up the new icons.")
