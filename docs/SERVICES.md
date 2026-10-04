# Services & Daemons Architecture

Pilot relies on three lightweight background services managed via `systemd --user`:

## 1. Remote Controller Daemon (`chromecast-remote.service`)

- **Script**: `remote_daemon.py`
- **Functionality**:
  - Connects to the Chromecast Bluetooth remote device via Linux `evdev`.
  - Exclusively grabs the input node to prevent unwanted duplicate events.
  - Automatically translates button presses into actions, media keys, or smooth mouse pointer coordinates.
  - Broadcasts live events over a local Unix Datagram socket (`pilot_ui.sock`) so the Settings UI can visually simulate button presses in real time.
  - Automatically reconnects when the remote goes to sleep or wakes up.

## 2. Voice Dictation Daemon (`chromecast-voice.service`)

- **Script**: `voice_daemon.py`
- **Functionality**:
  - Monitors the Assistant button press/release events.
  - Records audio chunks during the hold duration.
  - Runs local AI speech-to-text inference using `faster-whisper` (utilizing CUDA GPU acceleration if available, falling back to CPU).
  - Automatically types transcribed text into the active window or pastes it via clipboard.

## 3. ATVVoice BLE Audio Daemon (`atvvoice.service`)

- **Binary**: `bin/atvvoice`
- **Functionality**:
  - Communicates directly with the remote's Bluetooth Low Energy (BLE) GATT voice profile.
  - Streams ADPCM/Opus audio packets into a virtual PipeWire audio source for real-time dictation.
