"""Print the first available currency balances from a running WF Observer service."""

from __future__ import annotations

import argparse
import asyncio

import wf_observer as wf


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("endpoint", help="WF Observer service endpoint ID or ticket")
    return parser.parse_args()


async def currencies(endpoint: str) -> None:
    client = await wf.connect(endpoint)
    try:
        game = await client.warframe().single_session()
        balances = (await game.currencies().read()).balances
        print(f"Credits: {balances.credits}")
        print(f"Endo: {balances.endo}")
        print(f"Tradable Platinum: {balances.tradable_platinum}")
        print(f"Non-tradable Platinum: {balances.non_tradable_platinum}")

    finally:
        await client.shutdown()

def main() -> None:
    endpoint = arguments().endpoint
    asyncio.run(asyncio.wait_for(currencies(endpoint), timeout=30))


if __name__ == "__main__":
    main()
