# 0015. Commands and transactions

- Status: Accepted
- Requirements: CORE-06, CORE-11 (also CORE-07, API-05, ARCH-01, SEC-03)
- Issues: [#29](https://github.com/GuillaumeOj/BIMCraft/issues/29), [#30](https://github.com/GuillaumeOj/BIMCraft/issues/30), [#31](https://github.com/GuillaumeOj/BIMCraft/issues/31), [#32](https://github.com/GuillaumeOj/BIMCraft/issues/32)

## Context

Every change to a model goes through a command inside a transaction; transactions are atomic and undoable (CORE-06). Commands are serialisable, so history, macros, replay and collaboration share one mechanism (CORE-11). Each transaction reports what it changed (CORE-07), and the history records where each command came from (API-05).

The model is a `core_model::Model`: the attribute tables of [ADR 0007](0007-attribute-tables.md), relationships ([ADR 0012](0012-relationships.md)), property sets ([ADR 0013](0013-property-sets.md)), and components, types and instances ([ADR 0014](0014-components-types-instances.md)). Every `Model` write fails without changing anything and returns what it takes to revert it: `set`, `set_property` and `assign_type` the previous value, `remove` a `Removal` that `restore` puts back, `remove_participant` the position that `insert_participant` restores, `detach_property_set` the values and `remove_component` the component. Those ADRs leave to commands what deleting a type or a host means for the user. `core-txn` builds for `wasm32-unknown-unknown` (ARCH-01).

## Decision

- **A command is serialisable data.** `core_txn::Command` requires serde's `Serialize` and `DeserializeOwned`. It has a stable `name` and an `apply` method. A command carries every id it creates, chosen before it runs, so applying it is deterministic and a recorded log replays to the same model. The built-in commands form a closed enum, `ModelCommand`, with one command per `Model` write and a batch of these. Other command sets, such as plugins', are their own types implementing `Command`.
- **A transaction is a journal.** A command never sees the `Model` mutably. It writes through a `Transaction`, which offers the same writes as `Model`, forwards each one, and records a `Change` holding what the model returned. If `apply` fails, the transaction reverts its journal in reverse order, so the model is left exactly as it was. Adding a property set definition changes the schema, not the content, and is not a transaction write.
- **The history keeps journals.** A `Document` owns the model and two stacks of (command, journal) records. Undo reverts the journal. Redo makes the recorded changes again instead of running the command again, so it restores exactly the state before the undo. A new command clears the redo stack. Since the model changes only through the `Document`, every recorded change applies again. An error at that point would mean the model was changed behind the history's back.

## Consequences

- One mechanism covers rollback, undo and redo: each is a walk over the journal. Change events (#31) also need the objects as they were before the transaction, which the journal alone does not give for a value set and then set back.
- Replay needs only the commands, and the history keeps them in order (`Document::commands`).
- A journal holds the values and objects it overwrote or removed, so memory grows with the history. Bounding the history is future work.
- A new `Model` write needs a `Transaction` write and a `Change`, or commands cannot use it.
- Two decisions earlier ADRs leave to commands are not taken yet: what deleting a type or a host means for the user ([ADR 0012](0012-relationships.md), [ADR 0014](0014-components-types-instances.md)), and keeping `ref:` attribute values pointing at existing objects ([ADR 0007](0007-attribute-tables.md)). Both can be added as commands, or as checks over a transaction's journal before it ends, without changing this design.

## Alternatives considered

- **Journal raw `Store` writes.** Smaller, but a command could then bypass the relationship, property-set and type rules that `Model` enforces, and relationships, property sets and types could not be undone.
- **Trait objects for commands (`Box<dyn Command>`).** Serialising them needs a registry or a crate such as `typetag`, which relies on link-time registration that does not work on `wasm32-unknown-unknown`. A closed enum per command set is plain serde and builds everywhere.
- **Inverse commands instead of a journal.** Each command would have to compute its own inverse, a source of bugs for every new command. The journal works for any command.
- **Snapshots of the whole model.** Simple, but memory grows with the model size for every step of history.
- **Redo by running the command again.** Commands are deterministic, but a journal replay is exact by construction and needs no access to the command's logic.

## References

- [Specification §5](../../specifications.md), CORE-06, CORE-07, CORE-11; §12, API-05.
- [ADR 0007](0007-attribute-tables.md), attribute tables; [ADR 0012](0012-relationships.md), relationships; [ADR 0013](0013-property-sets.md), property sets; [ADR 0014](0014-components-types-instances.md), components, types and instances.
