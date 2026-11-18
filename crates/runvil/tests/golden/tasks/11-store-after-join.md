---
name: store-after-join
description: Reads a task's store write after joining it
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
tasks.join({ worker })
return store.read('notes.md')
```

## Worker

```lua
store.write('notes.md', 'written by the task')
return 'wrote'
```
