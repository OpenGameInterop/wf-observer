using WFObserver;

if (args.Length != 1)
{
    Console.Error.WriteLine("usage: dotnet run -- <endpoint-id-or-ticket>");
    Environment.ExitCode = 2;
    return;
}

var path = Path.Combine(Environment.GetFolderPath(Environment.SpecialFolder.UserProfile),
    ".wf-observer-examples", "csharp.key");
using var identity = Wf_observer_sdk.LoadIdentity(path);
Console.WriteLine($"Reader ID: {identity.EndpointId()}");
using var client = await identity.Connect(args[0]);
try
{
    using var warframe = client.Warframe();
    using var game = await warframe.SingleSession();
    using var currencies = game.Currencies();
    var balances = (await currencies.Read()).Balances;
    Console.WriteLine($"Credits: {balances.Credits}");
    Console.WriteLine($"Endo: {balances.Endo}");
    Console.WriteLine($"Tradable Platinum: {balances.TradablePlatinum}");
    Console.WriteLine($"Non-tradable Platinum: {balances.NonTradablePlatinum}");
}
finally
{
    await client.Shutdown();
}
