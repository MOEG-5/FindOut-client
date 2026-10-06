# FindOut website

The landing page lives in `site/` and is published at
[moeg-5.github.io/FindOut-client](https://moeg-5.github.io/FindOut-client/).
It is plain HTML and CSS, with no build step, JavaScript, analytics, or external
font requests. It uses the desktop app's Anton font and Monolith screenshot;
the font's license is included in `site/assets/`.

Preview it locally from the repository root:

```sh
python3 -m http.server 4174 --directory site
```

Open `http://localhost:4174`. All local asset paths are relative, so the same
files work under GitHub Pages' repository path.

GitHub Pages must use **GitHub Actions** as its publishing source in the
repository's Pages settings. `.github/workflows/pages.yml` publishes only
`site/` when that directory or the workflow changes on `main`. The workflow can
also be run manually. The PWA continues to be hosted separately on Vercel.

Download links use GitHub's `releases/latest/download/` URLs, so they follow new
releases without changing the page. Keep the release archive names stable.
When the desktop screenshot changes, copy `docs/findout-screenshot.png` to
`site/assets/findout.png` and update its dimensions and alt text if needed.
