# Host an always-on .anon site on bunny.net

This puts a small `.anon` site online around the clock without keeping your own
computer on. It uses bunny.net's **Magic Containers** with the Docker image this
repository builds automatically. No command line needed.

What you end up with: one small container that publishes the `site/` folder of
this repository at a fixed `.anon` address.

## 1. Let bunny download the image

Every push to `Master` builds the image and stores it on GitHub as
`ghcr.io/creepybits/iroh-socks-proxy`. You'll find it on the repository page under
**Packages** (right-hand side). Because the repository is private, the image is
private too, so bunny needs permission to download it. Pick one:

- **Easiest: make just the image public.** Open the package, click **Package
  settings**, scroll to **Danger Zone → Change visibility**, choose **Public**. The
  image contains only the compiled daemon and your `site/` files, not the
  repository, its history or the docs.
- **Keep it private:** on GitHub open your profile picture → **Settings → Developer
  settings → Personal access tokens → Tokens (classic) → Generate new token
  (classic)**, tick **read:packages**, generate, and copy the token. In bunny open
  **Magic Containers → Image Registries → Add Image Registry**, choose **GitHub**,
  enter `Creepybits` as the username and paste the token.

## 2. Create the app

In the bunny dashboard: **Magic Containers → Manage Apps → add / deploy a new app**.

| Setting | Value |
|---|---|
| Image | `ghcr.io/creepybits/iroh-socks-proxy`, tag `latest` |
| Regions | **one** region (any; pick one near you) |
| Scaling | **1 instance**, minimum 1 and maximum 1 (no autoscaling) |
| Persistent volume | add one, mount path **`/data`**, the smallest size offered |
| Command / arguments | leave empty |
| Environment variables | none |
| Endpoint | none needed (see below) |

Why one region and one instance: the volume holds the site's secret key, which
*is* its address. Every extra copy would get its own empty volume, make its own key
and so publish under a different address.

**Endpoint:** the daemon connects out to the Iroh network and visitors reach it
through Iroh, so nothing needs to be reachable from the web. If the dashboard won't
let you finish without one, add an **Anycast IP** endpoint (that also lets Iroh use
direct connections, which are faster). Never add a CDN/HTTP endpoint pointing at port
9999: that's the browser proxy and isn't meant to be public.

## 3. Find your address

Open the app's **logs** in bunny. After a few seconds you'll see:

```text
🔑 Your .anon address: <52 characters>.anon
📂 Publishing /site at that address.
```

That address is your site. Share it; anyone running the daemon with the extension ON
can open `http://<address>/`. It stays the same through restarts and redeploys as
long as the volume exists. **Deleting the volume means a new address.**

## 4. Change the page

Edit `site/index.html` (or add files to `site/`) on GitHub, using the pencil icon
and **Commit changes** to `Master`. GitHub builds a new image in a few minutes (see
the **Actions** tab). Then in bunny restart or redeploy the app so it pulls the new
`latest` image.

## If something doesn't work

- **The logs say "Iroh failed to start"**: the container can't reach the internet.
  Check the app's network settings.
- **The app keeps restarting with "permission denied" on `/data`**: check that the
  volume's mount path is exactly `/data`.
- **Visitors can't load the page**: check the logs still show the same address,
  and that visitors type `http://`, not `https://`.
