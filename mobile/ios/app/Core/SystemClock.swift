import Foundation

/// Clock trait 的宿主实现：系统时钟（core 不自取系统时间，TTL 判定经此注入）。
final class SystemClock: Clock {
    func nowUnixSecs() -> Int64 {
        Int64(Date().timeIntervalSince1970)
    }
}
