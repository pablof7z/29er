import SwiftUI
import UIKit

/// S02 nsec onboarding screen. Shown by `RootView` when `identityState` is
/// `.signedOut`, `.invalidKey`, or `.storageError`. The user pastes or types
/// an nsec and taps Sign In; `submitNsec` dispatches it to NMP (D004 — the
/// nsec is handed to Rust once and never re-read by Swift).
///
/// Deliberately plain — no Liquid Glass polish until S06. The error + loading
/// states are driven by `model.identityState` so the view is a pure function
/// of the model.
struct OnboardingView: View {
    @EnvironmentObject private var model: KernelModel
    @State private var nsecInput: String = ""
    @State private var hasAttemptedSubmit = false
    @FocusState private var fieldIsFocused: Bool

    private var isLoading: Bool {
        hasAttemptedSubmit && model.identityState == .unknown
    }

    private var isError: Bool {
        model.identityState == .invalidKey || model.identityState == .storageError
    }

    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                Spacer()

                VStack(spacing: 8) {
                    Image(systemName: "sailboat")
                        .font(.system(size: 56, weight: .light))
                        .foregroundStyle(.tint)
                    Text("29er")
                        .font(.largeTitle.bold())
                    Text("Sign in with your Nostr secret key to join groups.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .padding(.horizontal)
                }

                VStack(spacing: 12) {
                    SecureField("nsec1…", text: $nsecInput)
                        .textContentType(.password)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .focused($fieldIsFocused)
                        .submitLabel(.go)
                        .onSubmit(submit)

                    HStack(spacing: 12) {
                        Button("Paste nsec", action: paste)
                            .buttonStyle(.bordered)
                        Button("Sign In", action: submit)
                            .buttonStyle(.borderedProminent)
                            .disabled(nsecInput.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || isLoading)
                    }

                    if isLoading {
                        ProgressView("Signing in…")
                            .padding(.top, 4)
                    } else if isError {
                        Text(errorText)
                            .font(.callout)
                            .foregroundStyle(.red)
                            .multilineTextAlignment(.center)
                            .padding(.horizontal)
                    }
                }
                .padding(.horizontal)

                Spacer()
            }
            .navigationTitle("Welcome")
            .navigationBarTitleDisplayMode(.inline)
            .onAppear { fieldIsFocused = false }
        }
    }

    private var errorText: String {
        switch model.identityState {
        case .invalidKey:
            return "Invalid nsec — check the key starts with nsec1"
        case .storageError:
            return "Could not save the key — Keychain access failed"
        default:
            return ""
        }
    }

    private func submit() {
        let value = nsecInput
        guard !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return }
        hasAttemptedSubmit = true
        model.submitNsec(value)
        // D004 — clear the local input immediately so the nsec does not
        // linger in the text field state after dispatch.
        nsecInput = ""
    }

    private func paste() {
        if let s = UIPasteboard.general.string {
            nsecInput = s
            fieldIsFocused = true
        }
    }
}