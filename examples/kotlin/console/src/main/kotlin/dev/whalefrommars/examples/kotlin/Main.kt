package dev.whalefrommars.examples.kotlin

import dev.whalefrommars.wfobserver.WFObserver
import java.nio.file.Path

fun main(args: Array<String>) {
    require(args.size == 1) { "usage: kotlin-console <endpoint-id-or-ticket>" }
    val path = Path.of(System.getProperty("user.home"), ".wf-observer-examples", "kotlin.key")
    WFObserver.loadIdentity(path.toString()).use { identity ->
        println("Reader ID: ${identity.endpointId()}")
        identity.connect(args.single()).join().use { client ->
            try {
                client.warframe().use { warframe ->
                    warframe.singleSession().join().use { game ->
                        game.currencies().use { currencies ->
                            val balances = currencies.read().join().balances
                            println("Credits: ${balances.credits}")
                            println("Endo: ${balances.endo}")
                            println("Tradable Platinum: ${balances.tradablePlatinum}")
                            println("Non-tradable Platinum: ${balances.nonTradablePlatinum}")
                        }
                    }
                }
            } finally {
                client.shutdown().join()
            }
        }
    }
}
