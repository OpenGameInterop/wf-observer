import Darwin
import Foundation
import WFObserver

@main
enum WFObserverConsole {
    static func main() async throws {
        guard CommandLine.arguments.count == 2 else {
            FileHandle.standardError.write(Data("usage: WFObserverConsole <endpoint-id-or-ticket>\n".utf8))
            exit(2)
        }
        let client = try await WFObserver.connect(endpoint: CommandLine.arguments[1])
        do {
            let game = try await client.warframe().singleSession()
            let balances = try await game.currencies().read().balances
            print("Credits: \(balances.credits)")
            print("Endo: \(balances.endo)")
            print("Tradable Platinum: \(balances.tradablePlatinum)")
            print("Non-tradable Platinum: \(balances.nonTradablePlatinum)")
        } catch {
            try? await client.shutdown()
            throw error
        }
        try await client.shutdown()
    }
}
