# wongus

This is a desktop panel... bar thing. Make buttons, show window titles, add sliders and other widgets, so on and so forth.

I was trying `eww` when I realized I hated GTK's poor imitation of web standards even more than I hated web standards, and this was born.

This is basically just a webview with some functions for running commands. Design your desktop panel with the aesthetic sense of a 2024 web-developer. Use the modernest of CSSes. Let your inner marketer free and inject advertisements onto _your own_ desktop. Experience true freedom.

# Installation

Clone and `cargo build --release`, which will create `target/release/wongus`. You'll need the relevant webview library on your system (it'll tell you if you don't have something).

# Use

1. Create a directory for stuff like `/path/to/your/dir`
2. In the stuff directory, create `config.json` (actually see `src/main.rs` `struct Config` for the source, but hopefully I'll remember to keep this up to date):

   ```rust
   struct Config {
      /// Monitor to put the wongus on.
      #[serde(default)]
      monitor_index: Option<usize>,
      /// Monitor to put the wongus on. Any monitor with the model containing this string
      /// will match (case insensitive).
      #[serde(default)]
      monitor_model: Option<String>,
      /// Attach the top of the window to the top of the screen, stretching if the
      /// opposite is also attached.
      #[serde(default)]
      attach_top: bool,
      /// Attach the right of the window to the right of the screen, stretching if the
      /// opposite is also attached.
      #[serde(default)]
      attach_right: bool,
      /// Attach the bottom of the window to the bottom of the screen, stretching if the
      /// opposite is also attached.
      #[serde(default)]
      attach_bottom: bool,
      /// Attach the left of the window to the left of the screen, stretching if the
      /// opposite is also attached.
      #[serde(default)]
      attach_left: bool,
      /// If left or right aren't attached, specify the window width.
      #[serde(default)]
      width: Option<P2>,
      /// If top or bottom aren't attached, specify the window height.
      #[serde(default)]
      height: Option<P2>,
      /// Enable keyboard interaction (enables keyboard focus, required for keyboard
      /// interaction).
      #[serde(default)]
      enable_keyboard: bool,
      /// Window title.
      #[serde(default)]
      title: Option<String>,
      /// Http over unix domain socket for `curl`-based IPC.
      #[serde(default)]
      listen: Option<PathBuf>,
   }
   ```

3. Create `index.html` (and `style.css` and `script.js` and any other assets - you _must_ have `index.html` though), and add any other assets you want: images, fonts
4. Run `wongus /path/to/your/dir`

# Javascript API

Wongus adds a few things to `window` which are of particular relevance to panel bar thing designers.

## `wongus.args`

This is a `Map` containing key-value pairs from the command line (like `wongus config_dir/ k=v x=y`). If you do `wongus.args.get("k")` it will return `"v"` (all strings).

## `wongus.env`

This is a `Map` containing environment key-value pairs (all strings).

## `wongus.log`

Write a message to the stderr of wongus. Useful for external monitoring.

## `wongus.run_command`

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

## `wongus.stream_command`

```js
const res = await wongus.run_command({
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

# Caveats

- Due to https://github.com/tauri-apps/wry/issues/1255 pages are currently loaded with `filex://x` schema rather than `file://`. `filex` is like `file` but there's a `x` host that's ignored.

- There's a webkit issue https://github.com/tauri-apps/wry/issues/1252 with some Nvidia GPUs that makes them show a blank screen with compositing enabled. In my case, I think my GPU was working with proprietary drivers, and stopped working on Mesa (I needed Mesa for libvirt gpu acceleration though).
