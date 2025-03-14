
<p align="center">
<picture>
  <!-- Use the dark version if the user’s device or browser is set to dark mode -->
  <source srcset="https://github.com/user-attachments/assets/77253dda-5ed4-4aa0-aef5-c353aef1ccd9" media="(prefers-color-scheme: dark)"  width="200" margin="0"/>
  <!-- Fallback for light mode (or if no dark mode is detected) -->
  <source srcset="https://github.com/user-attachments/assets/123d7cc3-3658-4dab-95a9-a2c9cff5a471" media="(prefers-color-scheme: light)" width="200"  margin="0"/>
  <!-- In case neither condition is met -->
  <img src="https://github.com/user-attachments/assets/123d7cc3-3658-4dab-95a9-a2c9cff5a471" alt="Example image"  width="200" margin="0"/>
</picture>
</p>

#

# Webcam Direct Linux

This project is a Rust-based application that allows you to use your personal mobile as a webcam for your computer. It is inspired by the Apple Continuity feature but does not require iCloud. The provisioning process is done using BLE, and the application uses WebRTC for real-time video and audio streaming. The media processing and streaming are handled by GStreamer, and a virtual webcam device is created using v4l2loopback. The application also supports multiple cameras.

## Overview

Webcam Direct Linux is a Rust-based application designed to use your personal mobile as a webcam for your computer. Like Apple Continuity but without iCloud.

## Features

- P2P Wi-Fi connection between mobile and computer, fail back to LAN if not available.
- WebRTC for real-time video and audio streaming.
- GStreamer for media processing and streaming.
- v4l2loopback for creating a virtual webcam device.
- BLE for device discovery and proximity detection.
- MsgPack for efficient data transmission
- Multiple camera support.

## Getting Started

### Prerequisites

Dependencies:
```sh
sudo apt install libdbus-1-dev \
                             libgstreamer1.0-dev \
                             libgstreamer-plugins-base1.0-dev \
                             libgstreamer-plugins-bad1.0-dev \
                             gstreamer1.0-plugins-base \
                             gstreamer1.0-plugins-good \
                             gstreamer1.0-plugins-bad \
                             gstreamer1.0-plugins-ugly \
                             gstreamer1.0-tools \
                             gstreamer1.0-libav \
                             gstreamer1.0-libav \
                             libgstrtspserver-1.0-dev \
                             libges-1.0-dev \
                             gstreamer1.0-nice
```

### Installation

1. Clone the repository:
   ```sh
   git clone https://github.com/gamilr/webcam-direct-linux
   ```
2. Navigate to the project directory:
   ```sh
   cd webcam-direct-linux
   ```
3. Build the project:
   ```sh
   cargo build
   ```

## Usage

This process has to be run as root since it requires access to kernel Netlink, v4l2loopback and dbus.

Run the application:
```sh
sudo ./target/debug/webcam-direct-linux
```
