"""Check chat conversion through the generated binding without a service or game."""

import json
import unittest

import wf_observer as wf


class ChatBindings(unittest.TestCase):
    def test_directions_and_optional_peers_cross_native_conversion(self):
        metadata = wf.EnvelopeMetadata(
            source=wf.TopicSource(
                session=wf.SessionRef(run_id="run", session_id="session"),
                game_id="warframe",
                topic=wf.TopicRef(provider_id="opengameinterop.warframe",
                                  topic="warframe.chat", schema_version=1),
            ),
            generation="7",
            sequence="11",
        )
        account = "0123456789abcdef01234567"
        for direction in wf.ChatDirection:
            with self.subTest(direction=direction):
                sender = None if direction == wf.ChatDirection.SYSTEM else "Author"
                peer = "Someone" if direction in (
                    wf.ChatDirection.INCOMING, wf.ChatDirection.OUTGOING) else None
                message = {
                    "channel": "Direct",
                    "sender": sender,
                    "text": "<original markup>",
                    "game_time": {"hour": 12, "minute": 34},
                    "direction": direction.name.capitalize(),
                    "conversation_id": "opaque-conversation",
                    "peer": peer,
                }
                event = wf.WarframeChatEvent.from_envelope(wf.EventEnvelope(
                    metadata=metadata,
                    payload_json=json.dumps({"account_id": account,
                                             "update": {"Message": {"message": message}}}),
                ))
                self.assertEqual(event.metadata, metadata)
                self.assertEqual(event.account_id, account)
                self.assertIsInstance(event.update, wf.ChatUpdateMessage)
                self.assertEqual(event.update.value, wf.ChatMessage(
                    channel=wf.ChatChannel.DIRECT,
                    sender=sender,
                    text="<original markup>",
                    game_time=wf.ChatTime(hour=12, minute=34),
                    direction=direction,
                    conversation_id="opaque-conversation",
                    peer=peer,
                ))


if __name__ == "__main__":
    unittest.main()
