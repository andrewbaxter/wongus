# wongus

This is a desktop panel... bar thing. Make buttons, show window titles, add sliders and other widgets, so on and so forth.

I was trying `eww` when I realized I hated GTK's poor imitation of web standards even more than I hated web standards, and this was born.

This is basically just a webview with some functions for running commands. Design your desktop panel with the aesthetic sense of a 2024 web-developer. Use the modernest of CSSes. Let your inner marketer free and inject advertisements onto _your own_ desktop. Experience true freedom.

# Installation

Clone and `cargo build --release`, which will create `target/release/wongus`. You'll need the relevant webview library on your system (it'll tell you if you don't have something).

# Use

1. Create a directory for stuff
2. In the stuff directory, create `config.json` (actually see `src/main.rs` `struct Config` for the source, but hopefully I'll remember to keep this up to date):

   ```rust
   {
       /// Monitor to put the wongus on.
       monitor_index: Option<usize>,
       /// Monitor to put the wongus on. Any monitor with the model containing this string
       /// will match (case insensitive).
       monitor_model: Option<String>,
       /// Attach the top of the window to the top of the screen, stretching if the
       /// opposite is also attached.
       attach_top: bool,
       /// Attach the right of the window to the right of the screen, stretching if the
       /// opposite is also attached.
       attach_right: bool,
       /// Attach the bottom of the window to the bottom of the screen, stretching if the
       /// opposite is also attached.
       attach_bottom: bool,
       /// Attach the left of the window to the left of the screen, stretching if the
       /// opposite is also attached.
       attach_left: bool,
       /// If left or right aren't attached, specify the window width.
       width: Option<P2>,
       /// If top or bottom aren't attached, specify the window height.
       height: Option<P2>,
       /// Enable keyboard interaction (enables keyboard focus, required for keyboard
       /// interaction).
       enable_keyboard: bool,
   }
   ```

3. Create `index.html` (and `style.css` and `script.js` and any other assets - you _must_ have `index.html` though)
4. Run `wongus /path/to/your/dir`

# Javascript API

Wongus adds a few things to `window` of particular relevance to panel bar thing designers.

## `wongus.args`

This is a `Map` containing key-value pairs from the command line (like `wongus config_dir/ k=v x=y`). If you do `wongus.args.get("k")` it will return `"v"` (all strings).

## `wongus.env`

This is a `Map` containing environment key-value pairs (all strings).

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

# Caveats

- Due to https://github.com/tauri-apps/wry/issues/1255 pages are currently loaded with `filex://x` schema rather than `file://`. `filex` is like `file` but there's a `x` host that's ignored.

- There's a webkit issue https://github.com/tauri-apps/wry/issues/1252 with some Nvidia GPUs that makes them show a blank screen with compositing enabled. In my case, I think my GPU was working with proprietary drivers, and stopped working on Mesa (I needed Mesa for libvirt gpu acceleration though).
