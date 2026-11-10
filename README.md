# PiDE

**Pi Development Environment**: spin up a containerised [Pi](https://github.com/badlogic/pi-mono) coding agent for any project, backed by a local LLM such as Ollama or LM Studio.

PiDE targets a project folder (the current folder unless you give another), checks your container engine, asks which model to use, and drops you into Pi inside a container with the project mounted at `/workspace`. Everything it creates lives under `<project>/.pide/`.

## Requirements

- A container engine: [Podman](https://podman.io) (preferred), [Docker](https://www.docker.com), or [Apple's `container`](https://github.com/apple/container) (macOS with Apple silicon), installed and running
- A local LLM server reachable from the host, e.g. [Ollama](https://ollama.com) or [LM Studio](https://lmstudio.ai)
- Rust 1.85 or later, to build from source

> [!NOTE]
> `podman` and Docker map the host machine into each container automatically, so a local LLM
> server is reachable at `host.docker.internal` out of the box. Apple's `container` has no such
> flag: on macOS you must first make that name resolve, for example with
> `sudo container system dns create host.docker.internal --localhost 203.0.113.113` (see Apple's
> [host integration docs](https://github.com/apple/container/blob/main/docs/host-integration.md)).

## Install

```bash
cargo install --git https://github.com/argenkiwi/pide
```

## Usage

```bash
pide                 # set up and launch for the current folder
pide path/to/project # or for another folder
```

On the first run PiDE writes the base `Containerfile`, asks for your LLM server address and model, builds the image and launches Pi. Later runs reuse the existing config, so changing the server or model never needs an image rebuild.

### What PiDE creates

```text
.pide/
  Containerfile        Image definition; the /containerfile slash command adds project dependencies
  .gitignore           Keeps Pi's session history out of version control
  pi/
    models.json        One provider, pointing at your LLM server
    settings.json      Default provider and model
    prompts/           Slash commands, e.g. containerfile.md
    sessions/          Pi's session history
```

The full behaviour is in [`specs/pide.md`](specs/pide.md).

## How it is built

PiDE is a walk: a small engine (`src/engine.rs`) runs small nodes (`src/nodes/`) that are wired together by `src/walks/pide.rs`, with every side effect behind a utils trait so tests can mock it.

## Development

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## Releasing

PiDE follows [semantic versioning](https://semver.org). To release: update `version` in `Cargo.toml` and `CHANGELOG.md`, commit, then push a tag `vX.Y.Z`. The release workflow builds binaries and publishes a GitHub Release.

## Licence

MIT. See [`LICENSE`](LICENSE).
