-- Backlog: issues from outside wait here until a person approves them; trusted authors' issues and the issues of a
-- grilling the person confirmed move on.
-- A card that has not moved for more than `STALE_AFTER_DAYS` days is held and its people are told: a person releases it
-- (or closes the issue) to say what is to happen. Change the number in this script for a project that wants another time.
local STALE_AFTER_DAYS = 30

function on_card(card)
  if moebius.now() - card.moved_at > STALE_AFTER_DAYS * 86400 then
    moebius.notify("stale", "the card has not moved for " .. STALE_AFTER_DAYS .. " days")
    moebius.hold("stale: it has not moved for " .. STALE_AFTER_DAYS .. " days")
    return
  end
  moebius.set_label("in-progress", false)
  if card.trusted or card.confirmed then
    moebius.move("Not started")
  else
    moebius.ask("approve_issue", "waiting for a person to approve the issue")
  end
end
