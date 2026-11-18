---
name: join-two-tool-loops
description: Joins two tasks that each run a tool loop
runvil: 0
uses:
  - acme/echo
---

## Main

```lua
local first = tasks.spawn('../Worker', { item = 'first' })
local second = tasks.spawn('../Worker', { item = 'second' })
local results = tasks.join({ second, first })
return results[1].result .. ' | ' .. results[2].result
```

## Worker

```lua
tools.offer('echo/say')
messages:user('call echo_say {"text":"' .. item .. '"} then reply ' .. item .. ' done')
models.loop()
return messages[#messages].content
```
