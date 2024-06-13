document.addEventListener("DOMContentLoaded", async () => {
  const button = document.createElement("button");
  button.textContent = "Press";
  button.addEventListener("click", () => {
    wongus.run_command({ command: ["foot"] });
  });
  document.body.appendChild(button);
});
