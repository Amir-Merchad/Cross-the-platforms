import SwiftUI

@main
struct BridgeApp: App {
    @StateObject private var model = Model()

    var body: some Scene {
        WindowGroup { ContentView(m: model).onOpenURL { model.send([$0]) } }
    }
}
