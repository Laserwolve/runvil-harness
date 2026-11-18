---
name: return-with-waiting-task
description: Returns while a task waits after a reply
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
local _, _, reply = tasks.join_any({ worker })
return 'returned with ' .. reply
```

## Worker

```lua
tasks.reply('a draft')
return 'never reached'
```
