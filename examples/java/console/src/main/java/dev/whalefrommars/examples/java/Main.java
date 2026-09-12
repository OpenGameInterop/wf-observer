package dev.whalefrommars.examples.java;

import dev.whalefrommars.wfobserver.WFObserver;

public final class Main {
    private Main() {}
    public static void main(String[] args) {
        if (args.length != 1) {
            System.err.println("usage: java-console <endpoint-id-or-ticket>");
            System.exit(2);
        }
        try (var client = WFObserver.connect(args[0]).join()) {
            try (var warframe = client.warframe();
                 var game = warframe.singleSession().join();
                 var currencies = game.currencies()) {
                var balances = currencies.read().join().balances;
                System.out.println("Credits: " + balances.credits);
                System.out.println("Endo: " + balances.endo);
                System.out.println("Tradable Platinum: " + balances.tradablePlatinum);
                System.out.println("Non-tradable Platinum: " + balances.nonTradablePlatinum);
            } finally {
                client.shutdown().join();
            }
        }
    }
}
