![Pure beauty](./source/shot1.avif)
<small>(the thing on the left, not the right)</small>

# wongus

This is a Wayland desktop panel... bar thing. Make buttons, show window titles, add sliders and other widgets, so on and so forth.

I was trying `eww` when I realized I hated GTK's poor imitation of web standards even more than I hated web standards, and this was born.

This is basically just a webview with some functions for running commands. Design your desktop panel with the aesthetic sense of a 2024 web-developer. Use the modernest of CSSes. Let your inner marketer free and inject advertisements onto _your own_ desktop. Experience true freedom.

# Installation

Clone and `cargo build --release`, which will create `target/release/wongus`. You'll need the relevant webview library on your system (it'll tell you if you don't have something).

# Use

1. Create a directory for stuff like `/path/to/your/dir`

1. Put your `index.html` (and `style.css` and `script.js` and any other assets - you _must_ have `index.html` though) in it.

1. Add `config.json`, with contents like below:

   ```json
   {
     "attach_left": true,
     "attach_top": true,
     "attach_bottom": true,
     "width": {
       "logical": 150
     }
   }
   ```

   There's a schema for the config [here](./source/generated/jsonschema/config.schema.json). You can reference it in the root level field `$schema` to get VS Code autocomplete.

4. Run `wongus /path/to/your/dir`

There's an absolutely ugly example config dir in `example/` - try it out with `wongus ./example/`!

You can alternatively (instead of serving static files) serve content from a server using `--server http://127.0.0.1:8080`. In that case only the `config.json` is used from your config directory.

# Javascript API

This documentation might get out of sync - but you can use the provided [`wongus.d.ts`](./source/wongus.d.ts) file like:

```
/// <reference path="https://raw.githubusercontent.com/andrewbaxter/wongus/refs/heads/master/source/wongus.d.ts" />
```

Wongus adds a few things to `window` which are of particular relevance to panel bar thing designers.

## `wongus.args`

This is a `Map` containing key-value pairs from the command line (like `wongus config_dir/ k=v x=y`). If you do `wongus.args.get("k")` it will return `"v"` (all strings).

## `wongus.env`

This is a `Map` containing environment key-value pairs (all strings).

## `wongus.log`

Write a message to the stderr of wongus. Useful for external monitoring.

## `wongus.run_command`

This runs a short-lived command and returns its output, for instantaneous tasks like switching desktops or running a script to check the weather.

```js
const res = await wongus.run_command({
  command: ["echo", "hi"],
  working_dir: "/somewhere/over/the/rainbow", // Optional
  environment: { KEY: "value" }, // Optional
  timeout_secs: 10, // Optional, defaults to 10
});
console.log(res.stdout); // string
console.log(res.stderr); // string
```

## `wongus.run_detached_command`

This is the intended for spawning desktop applications like terminals, browsers, etc.

This runs a command as a child process and then throws away the handle. The child process will still be killed if `wongus` is, but nothing else about its lifecycle or result are managed. You may want to consider using `run_command` with `systemd-run` or something instead which will make the child fully independent, manage logs, capture exit status, etc.

```js
const res = await wongus.run_independent({
  command: ["echo", "hi"],
  working_dir: "/somewhere/over/the/rainbow", // Optional
  environment: { KEY: "value" }, // Optional
});
console.log(res.pid); // number
```

## `wongus.stream_command`

This runs a command and calls the callback whenever it writes a line of output to stdout.

```js
wongus.stream_command({
  command: ["echo", "hi"],
  cb: (line) => {
    // callback, called for each new line of output
    // `line` is a string
  },
  working_dir: "/somewhere/over/the/rainbow", // Optional
  environment: { KEY: "value" }, // Optional
});
```

## `wongus.read`

```js
const res = await wongus.read("/path/to/something");
console.log(res); // file contents, string
```

## `wongus.handle_external_ipc`

This is a callback for handling external IPC requests if you've enabled `listen` in the config.

```js
wongus.handle_external_ipc = (body) => {
  console.log(body);
  return {
    hi: "something",
  };
};
```

Communicate with the bar by doing `curl --unix-socket /path/from/config/listen http:/x --data '{"any": "json"}'` - the body will be passed to the callback and the return value will become the response body.

# Troubleshooting/debugging

If you right click on the panel and select "inspect element" it'll open the traditional web developer tools, where you can check requests, inspect the DOM, debug, peruse the console, etc.

# Is this a resource hog?

I don't know.

This is all theoretical, but:

- Since it uses the platform webview, if you have a webkit browser running it shouldn't load any more libraries into memory. This is the same argument for Tauri vs Electron
- Javascript hasn't changed _that_ much in 20 years, the javascript runtime and execution shouldn't be a significant drain (depending on your code of course) - compared to shell scripts or other bar config scripts
- DOM rendering vs DOM-like modern GTK rendering shouldn't be significantly different

So I'd _guess_ not. Keep in mind that getting memory usage statistics on linux is difficult, so if `top` says `VIRT` is `31.2g` that doesn't mean much.

On my system I didn't see a significant change in system resources when starting, but I have a lot running all the time so there's a lot of noise.

# Caveats, bugs, weird

- Due to https://github.com/tauri-apps/wry/issues/1255 pages are currently loaded with `filex://x` schema rather than `file://`. `filex` is like `file` but there's a `x` host that's ignored.

- There's a webkit issue https://github.com/tauri-apps/wry/issues/1252 with some Nvidia GPUs that makes them show a blank screen with compositing enabled. In my case, I think my GPU was working with proprietary drivers, and stopped working on Mesa (I needed Mesa for libvirt gpu acceleration though).

- An issue with screen blanking causing the window to disappear was just fixed in `gtk-layer-shell` (as of 2025/3/26) - it may be a bit before it's released and/or your distro packages it.

# Snippets

## An accurate clock

This also does things like show the date all Japanese-style and switch between local + UTC when you click it in various places.

You need to make `clockDate` and `clockTime` (text elements) to hold the time.

```javascript
const japanFormat = new Intl.DateTimeFormat("ja-JP", {
  dateStyle: "long",
  calendar: "japanese",
});
var dateWestern = true;
var timeLocal = true;
/**
 * @param {Date} now
 */
const redrawClock = (now) => {
  if (timeLocal) {
    if (dateWestern) {
      clockDate.textContent = `${now.getFullYear()}/${now.getMonth() + 1
        }/${now.getDate()}`;
    } else {
      clockDate.textContent = japanFormat.format(now);
    }
    clockTime.textContent = `${now
      .getHours()
      .toString()
      .padStart(2, "0")}:${now.getMinutes().toString().padStart(2, "0")}`;
  } else {
    clockDate.textContent = `${now.getUTCFullYear()}/${now.getUTCMonth() + 1
      }/${now.getUTCDate()}`;
    clockTime.textContent = `${now
      .getUTCHours()
      .toString()
      .padStart(2, "0")}:${now
        .getUTCMinutes()
        .toString()
        .padStart(2, "0")} Z`;
  }
};
clockDate.addEventListener("click", () => {
  timeLocal = true;
  dateWestern = !dateWestern;
  redrawClock(new Date());
});
clockTime.addEventListener("click", () => {
  timeLocal = !timeLocal;
  redrawClock(new Date());
});
const clock = () => {
  const now = new Date();

  // Shown time - we refresh slightly early, so show the next minute by ~200ms
  const showNow = new Date(now);
  showNow.setMilliseconds(now.getMilliseconds() + 200);

  // Schedule next update 100ms early to account for random delays.
  //
  // Time is calculated until the next shown time.
  const next = new Date(showNow);
  next.setMinutes(showNow.getMinutes() + 1, 0, 0);
  const untilNext = next.getTime() - now.getTime() - 100;
  setTimeout(() => clock(), untilNext);

  redrawClock(showNow);
};
clock();
```

## Fake transparent background

Blend effects don't work with normal wayland composition, but you can hack around it by taking a screenshot where the panel sits and using it as a background:

```javascript
const fakeBgPath = `${wongus.env.get("XDG_RUNTIME_DIR")}/wongus-shot.png`
if (!(await wongus.file_exists(fakeBgPath))) {
  await wongus.run_command({
    command: ["grim", "-g", `${window.screenX},${window.screenY} ${window.outerWidth}x${window.outerHeight}`, fakeBgPath],
  })
}
document.body.style.backgroundImage = `url('filex://x${fakeBgPath}')`
```