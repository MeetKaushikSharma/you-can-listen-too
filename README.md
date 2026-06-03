# YOU CAN LISTEN TOO :)

An ultra-low latency multi-device audio router built with Rust + Tauri. Clone your system's playback audio to up to 5 pairs of Bluetooth/USB headphones simultaneously! <(^_^)>

---

## What is this? (o_o)

When you want to share a movie, music, or game with friends using multiple Bluetooth/USB headphones on Windows, you usually hit a hardware wall. **YOU CAN LISTEN TOO** acts as a virtual audio splitter. 

It captures the system audio loopback in real time, downmixes multi-channel inputs, handles resamples dynamically to match each device's sample rate, and clones the output to every active receiver with per-device delay alignment.

---

## Features [=]

- **Multi-Device Support:** Route audio to up to 5 output devices at the same time. \-O-/
- **Dynamic Resampling:** Automatically adapts standard system outputs (e.g. 16-channel, stereo) to any headphone format.
- **Latency Calibration:** Adjust delay sliders (0ms - 500ms) on each receiver to perfectly sync headphones with different Bluetooth delays. :-D
- **Monochrome UI:** Clean, retro-console-inspired, high-contrast black-and-white theme. [^_^]

---

## Installation Tutorial [>]

Watch this [video tutorial](https://drive.google.com/file/d/1fvb9cVIUkvwGGgpChsVo7wluaeX0JIv5/view?usp=sharing) for step-by-step instructions on how to install the desktop app.

---

## How to Use :)

1. **Connect Headphones:** Connect all Bluetooth or USB headphones/earbuds to your PC.
2. **Setup Silent Routing (Recommended):**
   - Install a virtual audio device like **VB-Cable**.
   - Change your Windows default playback device to **CABLE Input (VB-Audio Virtual Cable)**.
3. **Configure the App:**
   - Open **YOU CAN LISTEN TOO**.
   - Select **CABLE Input (VB-Audio Virtual Cable)** in **00. INPUT SOURCE**.
   - Enable checkboxes on the right for your connected headphones and select their respective output devices.
4. **Start Listening:**
   - Click **START ROUTING**. 
   - Adjust individual volume levels and delay sliders to sync the audio.

---

## Tech Stack [=]

- **Frontend:** Vanilla HTML, CSS, JavaScript (via Tauri v2 wrapper)
- **Backend:** Rust (Tauri commands, `cpal` for low-level audio loopback capture/playback, `ringbuf` for lock-free audio buffers)

---

## Development & Build (^_^)

### Prerequisites
- Node.js & npm
- Rust toolchain (`stable-x86_64-pc-windows-msvc`)
- Visual Studio Build Tools (C++ tools)

### Install Dependencies
```bash
npm install
```

### Run Development Mode
```bash
npm run tauri dev
```

### Build Production Installer
```bash
npm run tauri build
```
*(Find your installer in `src-tauri/target/release/bundle/nsis/`)*

---

Enjoy listening together! :-D
