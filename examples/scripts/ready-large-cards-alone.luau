-- A Ready script: like the default, but a card whose title contains "[large]" starts only when no run is going,
-- so that it has the machine to itself. Install it as `.moebius/ready.luau` on the default branch.
function on_card(card)
  if #card.blocked_by > 0 then
    local numbers = {}
    for _, number in ipairs(card.blocked_by) do
      table.insert(numbers, "#" .. number)
    end
    moebius.wait("waiting for " .. table.concat(numbers, ", "))
    return
  end

  local limit = moebius.run_limit
  if string.find(card.title, "[large]", 1, true) then
    limit = 1
  end
  if moebius.runs_active() >= limit then
    moebius.wait("waiting for a free slot (limit " .. limit .. ")")
    return
  end

  moebius.move("In progress")
end
