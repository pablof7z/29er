import AVKit
import SwiftUI

/// Inline video playback for a `.video` media node, wrapping `AVKit.VideoPlayer`.
///
/// The `AVPlayer` is created exactly once per view identity, in `@State`'s
/// `initialValue` — NOT inline in `body`. `mediaGroup(urls:kind:)` in
/// `NostrContentView` runs on every SwiftUI re-render of the containing note
/// (scrolling, unrelated state changes, live timestamp refresh, etc.); a
/// player constructed directly in `body` was torn down and rebuilt — with a
/// full `AVPlayerViewController` KVO observer-registration churn — on every
/// single one of those re-renders, not just when the video URL actually
/// changed. This can saturate the main thread and make the app unresponsive
/// on any feed containing video content (same class of bug fixed in Chirp).
///
/// Second, independent guard: once `AVPlayerItem.status` reaches `.failed`
/// for this URL, playback stops being retried — a static fallback renders
/// instead, so one unloadable video URL can't burn CPU forever.
struct NostrInlineVideoPlayer: View {
    let url: URL
    @State private var player: AVPlayer
    @State private var failed = false

    init(url: URL) {
        self.url = url
        _player = State(initialValue: AVPlayer(url: url))
    }

    var body: some View {
        Group {
            if failed {
                fallback
            } else {
                VideoPlayer(player: player)
                    .aspectRatio(16.0 / 9.0, contentMode: .fit)
                    .clipShape(RoundedRectangle(cornerRadius: 10))
            }
        }
        .onChange(of: url) { _, newUrl in
            failed = false
            player = AVPlayer(url: newUrl)
        }
        .task(id: url) {
            guard !failed, let item = player.currentItem else { return }
            for await status in item.publisher(for: \.status).values {
                if status == .failed {
                    player.pause()
                    failed = true
                    return
                }
                if status == .readyToPlay {
                    return
                }
            }
        }
    }

    private var fallback: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 10)
                .fill(Color.gray.opacity(0.15))
            Image(systemName: "video.slash")
                .foregroundStyle(.secondary)
        }
        .aspectRatio(16.0 / 9.0, contentMode: .fit)
    }
}
