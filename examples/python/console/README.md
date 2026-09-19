# Python console example

Requires Python 3.10 or newer.

```bash
just example python --endpoint LOCAL_TICKET
```

After packaging the binding, install its wheel in an activated virtual environment
and observe chat for 30 seconds:

```bash
python -m pip install --no-index --find-links dist/python/wheelhouse wf-observer
python examples/python/console/main.py LOCAL_TICKET --chat
```

The example prints direction, author, private peer, original text and the scoped
conversation ID. It subscribes only to chat. Existing history is skipped, gap
notices need no recovery, and unknown identity remains `UNKNOWN`/`None`.

`just example python --check` also tests every chat direction and optional peer
through the generated native decoder.

See the [examples overview](../../README.md) for service setup, packaging and
the game-free `--check` mode.
