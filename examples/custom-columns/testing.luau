-- Ready for testing: the change waits here until a person has written the measurement into the issue, under a
-- heading `## Measurement`. Then the card goes on to Being measured.
local MARKER = "## Measurement"

function on_card(card)
  if string.find(card.body, MARKER, 1, true) == nil then
    moebius.wait("waiting for a " .. MARKER .. " section in the issue")
    return
  end
  moebius.move("Being measured", "the measurement is written in the issue")
end
