public enum LifecycleAction: Equatable { case none, start, stop, beginBackgroundTime, endBackgroundTime }

/// Decides when the node runs; the app performs the returned actions.
public struct NodeLifecycle {
    private var running = false
    private var inBackground = false
    private var bgTime = false
    private var pending = 0

    public init() {}

    public mutating func foreground() -> [LifecycleAction] {
        inBackground = false
        var a: [LifecycleAction] = []
        if !running { running = true; a.append(.start) }
        if bgTime { bgTime = false; a.append(.endBackgroundTime) }
        return a
    }

    public mutating func background() -> [LifecycleAction] {
        inBackground = true
        guard running else { return [] }
        if pending > 0 { if bgTime { return [] }; bgTime = true; return [.beginBackgroundTime] }
        return shutDown()
    }

    public mutating func transferStarted(files: Int) { pending += files }

    public mutating func fileFinished() -> [LifecycleAction] {
        pending = max(0, pending - 1)
        return pending == 0 && inBackground && running ? shutDown() : []
    }

    /// No transfer id is known, so a failure counts as one file; a stale count only holds the node until background time expires.
    public mutating func transferFailed() -> [LifecycleAction] {
        pending = max(0, pending - 1)
        return inBackground && running ? shutDown() : []
    }

    public mutating func backgroundTimeExpired() -> [LifecycleAction] {
        pending = 0
        return inBackground && running ? shutDown() : []
    }

    private mutating func shutDown() -> [LifecycleAction] {
        running = false
        var a: [LifecycleAction] = [.stop]
        if bgTime { bgTime = false; a.append(.endBackgroundTime) }
        return a
    }
}
