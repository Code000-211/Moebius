-- Being measured: a person gives the issue the label `measured` when the numbers are good, or `regressed` when they
-- are not. A good measurement offers the change to a person as the default review does; a bad one sends the card back
-- to coding with a note.
local function has_label(card, label)
  for _, own in ipairs(card.labels) do
    if own == label then
      return true
    end
  end
  return false
end

function on_card(card)
  if has_label(card, "regressed") then
    moebius.move("In progress", "The measurement regressed. Find out why, and fix it.")
  elseif has_label(card, "measured") then
    moebius.publish_change()
    moebius.offer_change()
  else
    moebius.wait("being measured: waiting for the label measured or regressed")
  end
end
