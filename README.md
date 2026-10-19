# ⛏️ mcserman (mc-server-manager)

> **A high-performance, host-independent Minecraft Dedicated Server Manager written in Rust.**

`mcserman` allows you to create, manage, configure, and supervise multi-instance Minecraft servers with zero external bloat. It provides interactive terminal wizards, native Unix domain socket supervision, progressive disclosure, automatic Java runtime resolution, Modrinth API integration, and full NixOS compatibility.

---

## ✨ Key Features

- 🦀 **Pure Rust & Static Binary:** Zero external script dependencies (no `tmux`, Python, or Node.js required).
- 🧭 **Progressive Disclosure:** Fully interactive prompts with fuzzy search when running in an interactive terminal, or 100% headless execution via command-line flags for scripts and `systemd`.
- 🔌 **Native Process Supervisor:** Launches Java as a child process and provides bi-directional control over a Unix Domain Socket with graceful shutdown (`SIGTERM` $\to$ `"stop\n"`).
- ☕ **Smart Java Resolver:** Automatic hierarchy resolution: Nix Store environment variables $\to$ System `$PATH` $\to$ Auto-download from Eclipse Temurin (Adoptium API) for any Linux distro.
- 📦 **Multi-Loader Support:** Automated provisioning for Mojang Vanilla, PaperMC (automatic latest build detection), and Fabric Loader.
- 🧩 **Modrinth Integration:** Search and download mods/plugins with concurrent streams and cryptographic SHA-512 verification.
- ❄️ **NixOS Native Flake:** Exports a reusable `packages.default` and a `nixosModules.default` with template unit `mcserver@<instance>.service`.

---

## 📥 Installation

### Method 1: On NixOS (Declarative via Flakes)

Add `mcserman` to your `flake.nix` inputs:

```nix
{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    mcserman.url = "github:AdrianChaSan/mcserman";
  };

  outputs = { self, nixpkgs, mcserman, ... }: {
    nixosConfigurations.my-host = nixpkgs.lib.nixosSystem {
      system = "x86_64-linux";
      modules = [
        ./configuration.nix

        # 1. Import the mcserman module
        mcserman.nixosModules.default

        # 2. Configure the service
        {
          services.mcserman = {
            enable = true;
            adminUser = "your_username"; # Optional: grants non-root management permissions
            openFirewall = {
              enable = true;
              port = 25565;
              interface = null;          # null = all interfaces, or e.g. "tailscale0"
            };
            javaVersions = [ 21 ];       # Pre-install Temurin JDK versions from Nix store
          };
        }
      ];
    };
  };
}
```

Then rebuild your system:
```bash
sudo nixos-rebuild switch
```

---

### Method 2: On Any Linux Distribution (Ubuntu, Debian, Fedora, Arch)

#### Option A: Pre-compiled Binary (Fastest)
Download the latest standalone executable from [GitHub Releases](https://github.com/AdrianChaSan/mcserman/releases/latest):

```bash
curl -sSL https://github.com/AdrianChaSan/mcserman/releases/latest/download/mcserman -o mcserman
chmod +x mcserman
sudo mv mcserman /usr/local/bin/
```

#### Option B: Via Cargo (`cargo install`)
If you have the Rust toolchain installed:
```bash
cargo install --git https://github.com/AdrianChaSan/mcserman
```

#### Option C: Build from Source
```bash
git clone https://github.com/AdrianChaSan/mcserman.git
cd mcserman
cargo build --release
sudo install -m 755 target/release/mcserman /usr/local/bin/
```

---

## 🚀 Usage Guide

### 1. Interactive Menu
Simply run `mcserman` without arguments in your terminal to open the interactive wizard:
```bash
mcserman
```

### 2. Creating an Instance
```bash
# Interactive wizard (prompts for loader, version, RAM, port)
mcserman create

# Non-interactive / Headless creation (for scripts & automated deployments)
mcserman create survival --loader paper --mc-version 1.21.4 --ram 4G --port 25565
```

### 3. Managing Instances
```bash
# List all registered instances and their online/offline state
mcserman list

# Start an instance in background
mcserman start survival

# View live console logs (colorized, follow mode)
mcserman logs survival -f

# Attach to interactive console (type commands in real time; ':q' or 'exit' to detach)
mcserman console survival

# Execute a single command without attaching
mcserman cmd survival "say Hello from mcserman!"
mcserman cmd survival "time set day"

# Stop the server cleanly (safely writes 'stop' to save the world)
mcserman stop survival

# Restart the instance
mcserman restart survival
```

### 4. Installing Mods & Plugins from Modrinth
Search and install compatible mods/plugins directly from the terminal with automatic SHA-512 verification:
```bash
mcserman add-mod survival
```

---

## 🛠️ Architecture & Under the Hood

### Native Process Supervisor (No `tmux` Required)
Instead of relying on external terminal multiplexers, `mcserman` runs a native daemon (`mcserman daemon <instance>`) that:
- Spawns the JVM as a managed child process.
- Binds a Unix Domain Socket (`$XDG_RUNTIME_DIR/mcserver/<instance>.sock` or `/run/mcserver/<instance>.sock`).
- Relays incoming IPC commands into the standard input of the Java process.
- Intercepts POSIX signals (`SIGTERM`, `SIGINT`) to send `"stop\n"` into Java's stdin, waiting up to 60 seconds for world consistency before terminating with exit code 0.

### Hybrid Java Resolution
When an instance starts (e.g. Minecraft 1.21.4 requiring Java 21), `mcserman` locates Java in the following order:
1. **Injected Environment Variables:** e.g. `MCSERVER_JAVA_21_PATH` (provided seamlessly by the NixOS module).
2. **System `$PATH`:** Checks `java -version` for a matching version.
3. **Automated Adoptium API Fallback:** If not found, downloads and unpacks the official Eclipse Temurin JDK for your architecture directly into `~/.local/share/mcserver/runtimes/`.

---

## 📜 License

MIT License. Developed by [AdrianChaSan](https://github.com/AdrianChaSan).
