"""Read currencies, or observe live chat with --chat, from a WF Observer service."""

from __future__ import annotations

import argparse
import asyncio
from pathlib import Path

import wf_observer as wf


def arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("endpoint", help="WF Observer service endpoint ID or ticket")
    parser.add_argument("--chat", action="store_true", help="observe chat for 30 seconds")
    return parser.parse_args()


async def show_chat(game: wf.WarframeSession) -> None:
    watch = await game.chat().watch()

    async def receive() -> None:
        while (item := await watch.next()) is not None:
            if isinstance(item, wf.ChatObservationMessage):
                message = item.value
                # The conversation ID is scoped by these existing envelope fields.
                scope = (item.metadata.source.session, item.account_id,
                         item.metadata.generation, message.conversation_id)
                print(f"{message.direction.name} {scope}: "
                      f"author={message.sender!r}, peer={message.peer!r}, "
                      f"text={message.text!r}", flush=True)

    try:
        await asyncio.wait_for(receive(), timeout=30)
    except asyncio.TimeoutError:
        pass
    finally:
        await watch.shutdown()


async def run(endpoint: str, chat: bool) -> None:
    identity = wf.load_identity(str(Path.home() / ".wf-observer-examples" / "python.key"))
    print(f"Reader ID: {identity.endpoint_id()}", flush=True)
    client = await identity.connect(endpoint)
    try:
        game = await client.warframe().single_session()
        if chat:
            await show_chat(game)
            return
        balances = (await game.currencies().read()).balances
        print(f"Credits: {balances.credits}")
        print(f"Endo: {balances.endo}")
        print(f"Tradable Platinum: {balances.tradable_platinum}")
        print(f"Non-tradable Platinum: {balances.non_tradable_platinum}")

    finally:
        await client.shutdown()


def main() -> None:
    args = arguments()
    asyncio.run(asyncio.wait_for(run(args.endpoint, args.chat), timeout=60 if args.chat else 30))


if __name__ == "__main__":
    main()
