# SharpClicker

A fast, dark-themed autoclicker for Windows, written in Rust with [egui](https://github.com/emilk/egui).

![SharpClicker screenshot](docs/screenshot.png)

## Features

- **Precise interval:** set hours, minutes, seconds and milliseconds. Default is 1 ms.
- **Mouse button choice:** left, right or middle.
- **Toggle key:** start and stop from any window. Default is **F6**, and you can rebind it in Settings.
- **Kill switch:** press **Delete** at any time to stop clicking and close the app immediately.
- **Backlog guard:** stops leftover clicks from continuing after you press stop. Adjustable from 1 to 100 ms.
- **Show above other apps:** optional, off by default.
- **Saved settings:** your settings are restored on launch.
- **Safety:** SharpClicker won't click while your cursor is over its own window.

## Download

Grab the latest `sharpclicker.exe` from the [Releases](../../releases) page.

## Build from source

You'll need:

- Windows 10 or 11
- [Rust](https://rustup.rs/) (run `rustup update` to get the latest version)
- Visual Studio Build Tools with the "Desktop development with C++" workload (needed for the Windows SDK and linker)

Then:

```bash
git clone https://github.com/YOUR-USERNAME/SharpClicker.git
cd SharpClicker
cargo run --release
```

The compiled program will be at `target/release/sharpclicker.exe`.

## Usage

1. Set the click interval using the Hours, Minutes, Seconds and Millis boxes.
2. Choose a mouse button.
3. Move your cursor to the spot you want to click. SharpClicker pauses while the cursor is over its own window.
4. Press **Start**, or your toggle key (F6 by default).
5. Press your toggle key again, or the **Stop** button, to stop.
6. Press **Delete** at any time to close the app immediately.

### Tips

- **Leftover clicks after stopping:** lower the Backlog guard value until leftover clicks start appearing, then raise it a little. Different apps can handle different click speeds.
- **Very fast intervals:** at 1 ms, the target app has to keep up with about 1,000 clicks per second. If it lags, raise the interval.
- **Changing the toggle key:** click the key button in Settings, press the new key, and it's saved automatically. Delete can't be used as the toggle key.

## Settings

Settings are saved to:

```
%APPDATA%\SharpClicker\settings.txt
```

Delete this file to reset everything to defaults.

## Responsible use

SharpClicker is meant for things like repetitive clicking tasks, accessibility, and testing your own software. Don't use it to gain an advantage in games or services that prohibit automation. Doing so can get your account banned, and you're responsible for following the rules of anything you use it with.

## Antivirus warnings

Some antivirus programs flag autoclickers because they send mouse input and read keyboard input. SharpClicker is open source, so you can review the code in `src/main.rs` yourself or build it from source. If Windows Defender flags the downloaded exe, it's a false positive. You can report it to Microsoft via [their submission page](https://www.microsoft.com/en-us/wdsi/filesubmission).

## Project structure

```
SharpClicker/
├── Cargo.toml     # dependencies and build settings
├── build.rs       # embeds the icon into the .exe
└── src/
    ├── main.rs    # all the app code
    └── icon.ico   # app and window icon
```

## Built with

- [eframe / egui](https://github.com/emilk/egui) for the UI
- [enigo](https://github.com/enigo-rs/enigo) for sending mouse clicks
- [device_query](https://github.com/ostrod/device_query) for global keyboard input
- [winresource](https://github.com/BenjaminRi/winresource) for embedding the icon

## License

MIT License

Copyright (c) 2026 outtee

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.