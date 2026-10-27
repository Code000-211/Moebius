-- Ready: starts work when nothing blocks the card, the daily token budget is not spent and the run limit allows.
function on_card(card)
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
