# VST3 MIDI Curves Build

## 🎯 Project Overview

Your project is now ready to build as a VST3 plugin and standalone application for all major platforms:

- **🪟 Windows** (x86_64)
- **🍎 macOS** (Intel + Apple Silicon Universal Binary)
- **🐧 Linux** (x86_64)

## 🏗️ Quick Start

### Prerequisites

1. **Rust** (version 1.70+)
   ```bash
   curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
   rustup update
   ```

2. **System dependencies**
   - **Windows**: `cross` for cross-compilation
   - **macOS**: XCode Command Line Tools
   - **Linux**: GTK4 development headers

### Build Commands

#### Automatic build (recommended)
```bash
# Build for current platform
./build.sh current

# Build for specific platform
./build.sh windows
./build.sh macos
./build.sh linux

# Build for all platforms
./build.sh all

# Clean and rebuild
./build.sh current --clean

# Command help
./build.sh help
```

#### Manual build
```bash
# VST3 plugin
cargo build --release --bin midi_curves_vst3

# Standalone application
cargo build --release --bin midi_curves
```

## 📋 Detailed Platform Instructions

### 🪟 Windows

#### Installing dependencies
```bash
# Install cross for cross-compilation
cargo install cross

# Verify installation
cross --version
```

#### Building
```bash
# Using script
./build_windows.sh

# Or manual build
cross build --release --target x86_64-pc-windows-msvc --bin midi_curves_vst3
cross build --release --target x86_64-pc-windows-msvc --bin midi_curves
```

#### Build Result
```
📂 target/x86_64-pc-windows-msvc/release/
├── midi_curves_vst3.dll        # VST3 plugin
└── midi_curves.exe             # Standalone application
```

#### VST3 Installation
1. Copy `midi_curves_vst3.dll` to plugins folder
2. VST3 installation paths for Windows:
   - `C:\Program Files\Common Files\VST3\`
   - `C:\Users\<username>\Documents\VST3\`

#### Standalone Installation
1. Copy `midi_curves.exe` to desired folder
2. Run the application

---

### 🍎 macOS

#### Installing dependencies
```bash
# XCode Command Line Tools
xcode-select --install

# Verify installation
xcode-select -p
```

#### Building
```bash
# Using script (recommended)
./build_macos.sh

# Script will create Universal Binary (Intel + Apple Silicon)
```

#### Build Result
```
📂 target/universal/
├── MidiCurves.vst3/            # VST3 plugin (Universal)
│   ├── Contents/
│   │   ├── Info.plist
│   │   ├── PkgInfo
│   │   └── MacOS/
│   │       └── MidiCurves      # Universal Binary
└── midi_curves                 # Standalone application (Universal)
```

#### VST3 Installation
```bash
# For all users (requires sudo)
sudo cp -r "MidiCurves.vst3" /Library/Audio/Plug-Ins/VST3/

# For current user
mkdir -p ~/Library/Audio/Plug-Ins/VST3/
cp -r "MidiCurves.vst3" ~/Library/Audio/Plug-Ins/VST3/
```

#### Standalone Installation
```bash
# Move to Applications
cp "midi_curves" /Applications/

# Set permissions (if needed)
chmod +x /Applications/midi_curves
```

#### macOS Security
When running for the first time:
1. System may show "cannot verify developer" warning
2. Open `System Preferences` > `Security & Privacy`
3. Click `Allow Anyway` for the application

---

### 🐧 Linux

#### Installing dependencies

**Ubuntu/Debian:**
```bash
sudo apt update
sudo apt install pkg-config libgtk-4-dev build-essential curl wget git
```

**Fedora:**
```bash
sudo dnf install pkg-config gtk4-devel gcc gcc-c++ curl wget git \
    jack-audio-connection-kit-devel mesa-libGL-devel mesa-libGLU-devel alsa-lib-devel
```

**Arch Linux:**
```bash
sudo pacman -S pkg-config gtk4 base-devel curl wget git
```

#### Building
```bash
# Using script
./build_linux.sh

# Or manual build
cargo build --release --bin midi_curves_vst3
cargo build --release --bin midi_curves
```

#### Build Result
```
📂 target/release/
├── midi_curves_vst3.so         # VST3 plugin
└── midi_curves                 # Standalone application
```

#### VST3 Structure for Linux
```bash
mkdir -p MidiCurves.vst3/Contents/x86_64-linux/
cp target/release/deps/libvst_midi_curves.so MidiCurves.vst3/Contents/x86_64-linux/MidiCurves.so
```

**IMPORTANT:** On Linux VST3 plugin must have `.so` extension. Many DAWs (including Reaper) do not recognize the plugin without this extension.

#### VST3 Installation
```bash
# For current user
mkdir -p ~/.vst3/
cp -r "MidiCurves.vst3" ~/.vst3/

# For all users (requires sudo)
sudo cp -r "MidiCurves.vst3" /usr/lib/vst3/
```

#### Running Standalone
```bash
# Direct run
./target/release/midi_curves

# With installed desktop file
# Find "MIDI Curves" in applications menu
```

## 🔧 Troubleshooting

### Common Issues

#### 1. Compilation errors
```bash
# Clear cache
cargo clean

# Update Rust
rustup update

# Rebuild
cargo build --release
```

#### 2. Missing dependencies
```bash
# Check system dependencies
# Windows: Install Visual Studio Build Tools
# macOS: xcode-select --install
# Linux: Check GTK4 and pkg-config
```

#### 3. Linking errors on Linux
```bash
# Ubuntu/Debian additional libraries
sudo apt install libssl-dev libasound2-dev libudev-dev libcairo2-dev libgdk-pixbuf2.0-dev

# Fedora additional libraries (JACK, OpenGL, ALSA)
sudo dnf install jack-audio-connection-kit-devel mesa-libGL-devel mesa-libGLU-devel alsa-lib-devel
```

### Specific Issues

#### Windows: "cross not found"
```bash
cargo install cross
```

#### macOS: "lipo not found"
```bash
xcode-select --install
```

#### Linux: "pkg-config not found"
```bash
# Ubuntu/Debian
sudo apt install pkg-config

# Fedora
sudo dnf install pkg-config

# Arch
sudo pacman -S pkg-config
```

## 📊 Testing

### VST3 Plugin Testing

1. **Compilation**: Check that compilation completes without errors
2. **DAW Loading**: Test in popular DAWs:
   - Reaper (cross-platform)
   - Ableton Live (Windows/macOS)
   - Logic Pro (macOS only)
   - FL Studio (Windows only)

3. **Functionality**: Check:
   - Preset loading and saving
   - MIDI velocity processing
   - GUI responsiveness
   - MIDI input/output

### Standalone Testing

1. **Launch**: Check application launch
2. **MIDI ports**: Check MIDI device detection
3. **Processing**: Test MIDI event processing
4. **GUI**: Check graph interactivity

## 🚀 Distribution

### Release Preparation

```bash
# Create release folder
mkdir -p releases/v0.1.0
cd releases

# Copy all platform builds
cp -r ../../target/x86_64-pc-windows-msvc/release/* windows/
cp -r ../../target/universal/* macos/
cp -r ../../target/release/* linux/

# Create archives
zip -r midi-curves-windows.zip windows/
zip -r midi-curves-macos.zip macos/
tar -czf midi-curves-linux.tar.gz linux/
```

### Distribution Recommendations

1. **Code signing** (for macOS and Windows)
2. **Versioning**: Use semantic versioning
3. **Changelog**: Keep a changelog
4. **Documentation**: Provide user manual

## 📚 Additional Resources

### Documentation
- [NIH-plug Book](https://nih-plug.robbert.vdh.org/)
- [VST3 SDK](https://steinbergmedia.github.io/vst3_doc/)
- [egui Documentation](https://docs.rs/egui/latest/egui/)

### Example Projects
- [NIH-plug Examples](https://github.com/robbert-vdh/nih-plug/tree/master/plugins)
- [Rust Audio Plugins](https://github.com/rust-audio/)

### Community
- [Rust Audio Discord](https://discord.gg/rust-audio)
- [NIH-plug Discussions](https://github.com/robbert-vdh/nih-plug/discussions)

---

**Document version:** 1.0  
**Date:** 2025-10-25  
**Status:** Ready for use