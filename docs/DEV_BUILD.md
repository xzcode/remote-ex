# Remote EX development test bundle

**Development preview, not a signed production installer or an unattended access service.** This bundle contains the app and three locally built media programs. The GStreamer runtime is NOT redistributed; install the official, architecture-matched GStreamer 1.26.11 runtime separately from https://gstreamer.freedesktop.org/download/ (choose old stable 1.26). On Windows, use the MSVC x86_64 runtime and ensure its bin directory is in PATH. On macOS, use the official universal runtime. Do not mix runtime architectures or third-party plugin trees. Dependencies and distribution obligations remain under review.

Extract all files into the same directory. Start remote-ex-desktop (Windows: remote-ex-desktop.exe). Do not move or replace any media program independently. The GUI supports encrypted view-only sharing (30-second limit) and diagnostics. Both peers must compare the complete displayed key bundle independently and explicitly approve. Keep the host UI visible and use Disconnect. Grant OS screen-recording permission normally if prompted; no elevated privileges or privacy bypass.

## Native control prototype
This iteration's real keyboard/mouse mode is a separate, visible, interactive terminal prototype, not enabled in the GUI. Requires native-control build, both owned/authorized devices, matching primary display coordinates, ordinary Screen Recording/Accessibility on macOS. From the extracted directory on the host:
```
./lan_control --listen PRIVATE_IP:PORT --control DISPLAY_ID
```
On the controller:
```
./lan_control --connect PRIVATE_IP:PORT --control
```
Windows PowerShell: use .\lan_control.exe instead. Select the host primary display ID using the app's local display diagnostics. Independently verify both complete key bundles; the host explicitly confirms screen AND input, the controller explicitly confirms input. Approve host first. The host must keep the visible terminal; Ctrl-C revokes, Escape or closing the controller window disconnects. Native window forwards input only when focused and within the picture content. Do not expose the listener to the Internet or enable port forwarding. Sessions are limited to 30 seconds, no automatic reconnect or unattended control. Normal stop requests release tracked keys/buttons before acknowledgement; sudden kill, OS failure, or a blocked platform capture can delay or prevent cleanup. Do not rely on this prototype for critical systems.

CI verifies synthetic/inert tests only. Actual physical-device display capture, GUI, key mapping/DPI, and native input behavior still require authorized local testing. The Windows binaries are unsigned, macOS binaries only ad-hoc signed; no notarization or release. Do not bypass OS security policies. Development build uses GStreamer 1.26.11 SDK for reproducible testing, not a security-audited frozen release configuration.
