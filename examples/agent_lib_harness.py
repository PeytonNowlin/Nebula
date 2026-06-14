"""Python harness for examples/agent_lib.neb.

Demonstrates the "author in Nebula, ship as Python" integration path: import the
compiled package and call Nebula-authored sector functions directly, getting
native Python values back. Driven by the `nebula_library_is_callable_from_python`
integration test; mirrors docs/author-in-nebula-ship-as-python.md.

Usage: python agent_lib_harness.py <compile --out dir>
"""

import sys

# The directory passed by the test is `nebula compile --target python --out <dir>`.
sys.path.insert(0, sys.argv[1])

import agent_lib  # noqa: E402  (path is set up above)

# Call typed Nebula logic from plain Python — values come back as native types.
assert agent_lib.text.field("alice,30,engineer", 0) == "alice"
assert agent_lib.text.field("alice,30,engineer", 2) == "engineer"

count = agent_lib.text.field_count("a,b,c,d")
assert count == 4 and isinstance(count, int), count

assert agent_lib.text.has_token("id,name,age", "name") is True
assert agent_lib.text.has_token("id,name,age", "missing") is False

print("agent_lib harness ok")
