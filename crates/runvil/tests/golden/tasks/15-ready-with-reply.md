---
name: ready-with-reply
description: Reads tasks.ready on a task that replied and stays open
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
local other = tasks.spawn('../Other')
tasks.join({ other })
log('reply waiting: ' .. tostring(tasks.ready(worker)))
local _, _, reply = tasks.join_any({ worker })
log('reply taken: ' .. tostring(tasks.ready(worker)))
tasks.cancel(worker)
return reply
```

## Worker

```lua
tasks.reply('a draft')
return 'never reached'
```

## Other

```lua
return 'other'
```
