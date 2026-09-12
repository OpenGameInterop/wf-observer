package dev.whalefrommars.examples.kotlin

import dev.whalefrommars.wfobserver.WFObserver

fun main(args: Array<String>) {
    require(args.size == 1) { "usage: kotlin-console <endpoint-id-or-ticket>" }
    WFObserver.connect(args.single()).join().use { client ->
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
