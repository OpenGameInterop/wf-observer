package dev.whalefrommars.examples.java;

import dev.whalefrommars.wfobserver.WFObserver;
import java.nio.file.Path;

public final class Main {
    private Main() {}
    public static void main(String[] args) {
        if (args.length != 1) {
            System.err.println("usage: java-console <endpoint-id-or-ticket>");
            System.exit(2);
        }
        var path = Path.of(System.getProperty("user.home"), ".wf-observer-examples", "java.key");
        try (var identity = WFObserver.loadIdentity(path.toString())) {
            System.out.println("Reader ID: " + identity.endpointId());
            try (var client = identity.connect(args[0]).join()) {
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
}
