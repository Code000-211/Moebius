-- Not started: starts work when nothing blocks the card, the daily token budget is not spent and the run limit allows.
-- An issue labelled `wayfinder:...`, in any case (a wayfinder map or one of its tickets) is a question, not a task: it starts no work and
-- stays here until the wayfinder session that works it closes it.
-- An issue with sub-issues is a parent: it starts no work of its own while one is open, and closes with the last one.
-- A parent that has work of its own already (a run, or a change) goes on as any other card does.
function on_card(card)
  moebius.set_label("in-progress", false)
  for _, label in ipairs(card.labels) do
    if string.sub(string.lower(label), 1, 10) == "wayfinder:" then
      moebius.wait("a wayfinder question, not a task: it stays until the wayfinder session closes it")
      return
    end
  end
  if #card.children > 0 and card.run == nil and card.change == nil then
    local open = {}
    for _, child in ipairs(card.children) do
      if child.state == "open" then
        table.insert(open, "#" .. child.number)
      end
    end
    if #open > 0 then
      moebius.wait("waiting for sub-issues " .. table.concat(open, ", "))
    else
      moebius.close_issue("every sub-issue is done")
    end
    return
  end
  if #card.blocked_by > 0 then
    local numbers = {}
    for _, number in ipairs(card.blocked_by) do
      table.insert(numbers, "#" .. number)
    end
    moebius.wait("waiting for " .. table.concat(numbers, ", "))
    return
  end
  if moebius.day_budget ~= nil and moebius.tokens_today() >= moebius.day_budget then
    moebius.wait("the daily token budget is spent")
    return
  end
  if moebius.runs_active() >= moebius.run_limit then
    moebius.wait("the run limit is reached")
    return
  end
  moebius.move("In progress")
end
