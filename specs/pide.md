# Program Specifications

Pide (Pi Development Environment) quickly sets up a containerised development environment for a code project. Inside the container, the Pi coding agent talks to a local LLM server such as Ollama or LM Studio. Pide targets a project folder, which is the current folder unless another one is given, and keeps everything it creates under `<target>/.pide/`:

- `Containerfile`: the image definition. It starts with the base layers needed to run Pi, and the `/containerfile` slash command adds the project's own dependencies to it.
- `.gitignore`: keeps Pi's session history out of version control.
- `pi/`: Pi's agent configuration, mounted into the container, so changing the server or model never requires an image rebuild.
  - `models.json`: one provider, pointing at the LLM server and listing the selected model.
  - `settings.json`: the default provider and model.
  - `prompts/containerfile.md`: the `/containerfile` slash command.
  - `sessions/`: Pi's session history, written by Pi while it runs.

## Nodes

### Resolve Target
- Initial node of the application.
- Reads the optional target folder argument and uses the current folder if none is given.
- If the folder exists, it records its absolute path and transitions to `DETECT_RUNTIME`.
- If the folder does not exist, it records an error and transitions to `FAIL`.

### Detect Runtime
- Picks the container engine: the first of `podman`, `docker` and Apple's `container` that is both installed and running. An installed engine that is not running is skipped in favour of the next one.
- Checks that an engine is running and responding. For `podman` and `docker` this is the `info` command; for `container` it is `system status`.
- If no engine is installed, it records an error with installation guidance and transitions to `FAIL`.
- If engines are installed but none is running, it records an error for the most-preferred installed one, with a hint on starting it (`podman machine start`, starting Docker Desktop, or `container system start`), and transitions to `FAIL`.
- If the engine is ready, it transitions to `ENSURE_CONTAINERFILE`.

### Ensure Containerfile
- Checks for `.pide/Containerfile` in the target folder.
- If it is missing, it writes the base Containerfile and informs the user. The base image:
  - starts from a slim Debian image with the Node runtime;
  - installs git, ca-certificates, curl, ripgrep and fd;
  - installs the Pi coding agent globally;
  - marks `/workspace` as a safe git directory, because the mounted project is owned by the host user;
  - uses `/workspace` as its working directory and runs Pi as its entry point.
- If `.pide/.gitignore` is missing, it writes one that ignores `pi/sessions/`.
- Leaves existing files untouched.
- Transitions to `CHECK_CONFIG`.

### Check Config
- Reads `.pide/pi/models.json` and `.pide/pi/settings.json`.
- If both files exist, define a provider with a server URL, and name a default model belonging to that provider, it records the URL and model and transitions to `ENSURE_PROMPT`.
- If either file is missing, unreadable or incomplete, it informs the user that setup is needed and transitions to `ENTER_HOST`.

### Enter Host
- Prompts the user for the LLM server URL, with Ollama (`http://localhost:11434`) and LM Studio (`http://localhost:1234`) shown as examples.
- If the input is empty, it uses the Ollama default and transitions to `FETCH_MODELS`.
- If the URL is valid, it records it and transitions to `FETCH_MODELS`.
- If the URL is malformed, it displays an error message and transitions to `ENTER_HOST`.
- If input ends without a URL, it records an error and transitions to `FAIL`.

### Fetch Models
- Requests the list of available models from the server's OpenAI-compatible endpoint (`<url>/v1/models`).
- If the server cannot be reached or returns an error, it displays the error and transitions to `ENTER_HOST`.
- If the server reports no models, it suggests pulling or loading one and transitions to `ENTER_HOST`.
- If models are available, it records them and transitions to `SELECT_MODEL`.

### Select Model
- Displays the available models as a numbered list and prompts the user to pick one.
- If the input is a number in range, it records the selected model and transitions to `SAVE_CONFIG`.
- If the input is not a number or is out of range, it displays an error message and transitions to `SELECT_MODEL`.
- If input ends without a choice, it records an error and transitions to `FAIL`.

### Save Config
- Derives the URL the container uses to reach the server: `localhost` and `127.0.0.1` are replaced with `host.docker.internal`.
- Writes `.pide/pi/models.json` with a single provider. The provider uses the container URL, the OpenAI-compatible completions API and a placeholder API key, has developer-role messages disabled (which Ollama requires), and has zero cost for the selected model.
- Writes `.pide/pi/settings.json`, making that provider and model the defaults.
- Transitions to `ENSURE_PROMPT`.

### Ensure Prompt
- Checks for `.pide/pi/prompts/containerfile.md`.
- If it is missing, it writes the prompt behind the `/containerfile` slash command. The prompt instructs Pi to:
  - inspect the project's manifests, lockfiles and toolchain files (for example `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod`, `.tool-versions`);
  - add the toolchains, system packages and tools the project needs to `.pide/Containerfile`, keeping the base layers that Pi depends on;
  - match versions the project already declares;
  - tell the user to exit and run pide again so the image is rebuilt.
- Leaves an existing prompt untouched, so the user can customise it.
- Transitions to `BUILD_IMAGE`.

### Build Image
- Derives the image name from the target folder's name plus a short hash of its absolute path, so different projects with the same folder name don't share an image.
- Computes a hash of the Containerfile's contents and compares it with the one labelled on the existing image, if any. `podman` and `docker` read the label with `image inspect --format`; Apple's `container` prints `image inspect` as JSON, which pide parses instead.
- If the hashes match, it skips the build and transitions to `LAUNCH`.
- Otherwise, it builds the image (showing the build output) and labels it with the new hash. The `build` flags (`-t`, `-f`, `--label`, and the positional context) are the same across all three engines.
- If the build succeeds, it transitions to `LAUNCH`.
- If the build fails, it records the error and transitions to `FAIL`.

### Launch
- Runs the image as an interactive container, using a terminal when one is attached, and removes the container when it exits.
- Mounts the target folder at `/workspace` and `.pide/pi` as Pi's agent configuration folder, using the `:z` flag so shared SELinux labels permit container writes. Apple's `container` rejects `:z` (the mount fails to start), so it gets plain mounts.
- Maps `host.docker.internal` to the host so the container can reach the LLM server. `podman` and `docker` do this with `--add-host host.docker.internal:host-gateway`; Apple's `container` has no such flag, so the mapping is left to the user (a `container` container must reach `host.docker.internal` some other way).
- Passes the host's git author and committer name and email to the container.
- Disables Pi's telemetry and update check.
- Waits for Pi to exit.
- If the container exits normally (whatever Pi's own exit code), it transitions to `FINISH`.
- If the container cannot be started or exits with an engine error, it records the error and transitions to `FAIL`.

### Finish
- Termination node.
- Displays the project and model that were used, and a reminder that changes made with `/containerfile` take effect on the next run.
- Terminates successfully.

### Fail
- Termination node.
- Displays the recorded error.
- Terminates unsuccessfully.

## Shared State

The shared state consists of:
- `target`: The absolute path of the project folder.
- `runtime`: The container engine in use (`podman`, `docker`, or Apple's `container`).
- `host_url`: The LLM server URL as the user entered it.
- `container_url`: The same URL as seen from inside the container.
- `models`: The model identifiers reported by the server.
- `model`: The selected model identifier.
- `image`: The name of the project's image.
- `error`: The message displayed by `FAIL`.
