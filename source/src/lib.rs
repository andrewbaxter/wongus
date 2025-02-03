use {
    schemars::JsonSchema,
    serde::Deserialize,
    std::path::PathBuf,
};

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum P2 {
    /// Not pixels, but a delusion that will become a pixel once a scaling factor is
    /// applied.
    Logical(i32),
    /// Percent of monitor size (0-100).
    Percent(f64),
}

#[derive(Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct Config {
    #[serde(rename = "$schema", skip_serializing)]
    pub _schema: Option<String>,
    /// Monitor to put the wongus on.
    #[serde(default)]
    pub monitor_index: Option<usize>,
    /// Monitor to put the wongus on. Any monitor with the model containing this string
    /// will match (case insensitive).
    #[serde(default)]
    pub monitor_model: Option<String>,
    /// Attach the top of the window to the top of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    pub attach_top: bool,
    /// Attach the right of the window to the right of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    pub attach_right: bool,
    /// Attach the bottom of the window to the bottom of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    pub attach_bottom: bool,
    /// Attach the left of the window to the left of the screen, stretching if the
    /// opposite is also attached.
    #[serde(default)]
    pub attach_left: bool,
    /// If left or right aren't attached, specify the window width.
    #[serde(default)]
    pub width: Option<P2>,
    /// If top or bottom aren't attached, specify the window height.
    #[serde(default)]
    pub height: Option<P2>,
    /// Enable keyboard interaction (enables keyboard focus, required for keyboard
    /// interaction).
    #[serde(default)]
    pub enable_keyboard: bool,
    /// Window title.
    #[serde(default)]
    pub title: Option<String>,
    /// Http over unix domain socket for `curl`-based IPC.
    #[serde(default)]
    pub listen: Option<PathBuf>,
}
