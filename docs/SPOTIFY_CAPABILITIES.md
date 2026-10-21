# Spotify capability matrix

Riff uses `librespot 0.8.0` as its playback and Spotify Connect foundation. This document records which client capabilities are available through the APIs Riff already depends on, which need additional state plumbing, and which would require a separate provider/API strategy.

The goal is to keep the Workbench honest: unsupported actions should stay hidden or disabled instead of appearing as buttons that fail mysteriously.

## Native through `librespot::connect::Spirc`

These controls are exposed directly by the current stable `Spirc` handle and can be represented as reusable Riff actions without adding another Spotify API dependency.

| Capability | Current upstream primitive | Riff status |
| --- | --- | --- |
| Play | `Spirc::play()` | supported |
| Pause | `Spirc::pause()` | supported through play/pause flow |
| Play / pause toggle | `Spirc::play_pause()` | supported |
| Previous track | `Spirc::prev()` | supported |
| Next track | `Spirc::next()` | supported |
| Seek | `Spirc::set_position_ms()` | supported |
| Absolute volume | `Spirc::set_volume()` | supported |
| Volume step | `Spirc::volume_up()` / `volume_down()` | available upstream; Riff uses its own stable 5% absolute-volume steps |
| Shuffle | `Spirc::shuffle(bool)` | supported |
| Repeat context | `Spirc::repeat(bool)` | supported |
| Repeat current track | `Spirc::repeat_track(bool)` | available upstream; not yet surfaced by Riff |
| Load playback context | `Spirc::load()` | supported |
| Activate local Connect device | `Spirc::activate()` | supported |
| Disconnect local Connect device | `Spirc::disconnect()` | available upstream |

## Native primitive, but more state is required

### Playback transfer

`Spirc::transfer(Option<TransferRequest>)` exists, so librespot can participate in Spotify Connect transfer flows. However, the public `Spirc` handle does not provide a simple device-list API that Riff can directly turn into a reliable picker.

Before exposing device switching, Riff needs a source of truth for available/active Connect devices, stable target identifiers, and remote state changes. A transfer button without that state would be misleading.

### Connection / device status

The player event stream exposes `SessionConnected` and `SessionDisconnected`, and `Spirc::activate()` emits the connected event. Riff can therefore show real local Connect-session state without guessing from process lifetime.

This is suitable for a footer/header status indicator and should also be reusable by future device controls.

## Not exposed as direct `Spirc` controls

The following features should not be presented as native librespot controls in Riff today:

| Capability | Current situation |
| --- | --- |
| Save / like current track | no straightforward public `Spirc` action |
| Remove saved / unlike | no straightforward public `Spirc` action |
| Browse saved library | no high-level `Spirc` library API |
| List Spotify Connect devices | no simple public device-list getter on the `Spirc` handle |
| Inspect/edit the remote Spotify queue | no simple public queue getter/editor on the `Spirc` handle |
| Set autoplay | Spirc observes relevant user-attribute changes internally, but does not expose a direct public autoplay setter |

These may be possible through other internal Spotify endpoints or the Spotify Web API, but that is a separate capability with different authentication, stability and quota considerations. Core playback must not silently become dependent on it.

## Genre metadata

The high-level librespot `Artist` metadata model does not expose genres. The protocol does contain extended track descriptor machinery, including `TRACK_DESCRIPTOR`, descriptor names/weights and descriptor types such as `GENRE`.

That makes provider-native genre enrichment worth experimenting with, but it is not yet a proven high-level API. Riff should validate the descriptor endpoint on real music tracks before treating it as stable genre metadata. Genres must never be guessed from title, album or artist text.

## Queue and incremental resolution

Riff currently uses librespot `0.8.0`, which does not expose the safe dynamic queue-append API available in newer upstream development code. `Spirc::load()` replaces/loads playback context and is not an acceptable substitute for append because it can disturb active playback.

Riff should remain on a released, reproducible librespot dependency. Incremental queue append stays blocked until a safely pinnable release exposes the required primitive or a separately justified dependency upgrade is made.

## Implementation order

For the current v0.7 work, the lowest-risk client-capability sequence is:

1. expose repeat-current-track through the existing Action → Control → Spirc path;
2. expose real Connect session status from player events;
3. investigate reliable device discovery/state before adding transfer UI;
4. validate extended track descriptors for optional genre enrichment;
5. only then evaluate Web API/internal endpoint additions for library/save features.

Keep capability-specific failures isolated from local playback. If an optional Spotify enrichment service is unavailable, the player should continue to work normally.
