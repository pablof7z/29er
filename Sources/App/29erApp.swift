import SwiftUI

@main
struct TwentyNineApp: App {
    var body: some Scene {
        WindowGroup {
            ContentView()
                .environmentObject(KernelModel())
        }
    }
}
