# Eclipse integration

Pinned source: a76e030d749ef558e687bbff6dc592a7dddec244 from ViceVerse-cz/Serein.

Copied the independent `discord-voice` crate and platform video decoder source, retaining the MIT and Apache-2.0 licenses. Minimal local `client-core` and `model` adapters expose only voice credentials, IDs, voice processing and screen settings. The platform adapter includes native video decoding, without Serein's application, web interface, notifications or storage.

Eclipse changes `video_receive::MAX_DECODERS` from 8 to 4 to bound active video memory. Metadata still admits 16 announced sources. Per-user mixing and stream playout accept up to 1,000% volume, with final PCM clipping and bounded queues; updated mixer and playout tests exercise this behavior. Received video is put back in sequence order before reassembly (`video_receive::Reorder`): a gap is requested again with RFC 4585 generic NACKs every 60 ms (up to four times) and later packets wait up to 250 ms for it, so a lost packet no longer discards every picture until the next keyframe; unrecovered gaps fall back to the original keyframe request. Both voice and stream transports send these NACKs. Other media-engine source is unchanged. The davey and hpke-rs dependency patches are upstream Serein's pinned patches, with original source and notices supplied.

The Windows libopus CRT fix is maintained separately in `../libopus_sys/ECLIPSE-PATCH.md`.

Eclipse's own `src/calls.rs` handles call intent, Gateway events, capture/device lifetimes, frame budgets and the native UI. Upstream protocol tests are local fixtures, not proof of live Discord interoperability.
