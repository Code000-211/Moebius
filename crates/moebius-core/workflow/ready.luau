-- Not started: starts work when nothing blocks the card, the daily token budget is not spent and the run limit allows.
-- An issue with sub-issues is a parent: it starts no work of its own while one is open, and closes with the last one.
-- A parent that has work of its own already (a run, or a change) goes on as any other card does.
function on_card(card)
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
