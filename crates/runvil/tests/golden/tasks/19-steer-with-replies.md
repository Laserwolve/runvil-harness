---
name: steer-with-replies
description: Steers a worker with two messages between its replies
runvil: 0
---

## Main

```lua
local worker = tasks.spawn('../Worker')
local _, _, draft = tasks.join_any({ worker })
log('first reply: ' .. draft)
tasks.send(worker, 'shorten it')
local _, _, shorter = tasks.join_any({ worker })
log('second reply: ' .. shorter)
tasks.send(worker, 'done')
local results = tasks.join({ worker })
return results[1].result
```

## Worker

```lua
local message = tasks.reply('a long draft')
log('worker got: ' .. message)
message = tasks.reply('a short draft')
log('worker got: ' .. message)
return 'finished after ' .. message
```
