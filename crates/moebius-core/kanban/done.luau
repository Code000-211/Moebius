-- Done: every judgement made for the card learns how it ended: merged when its change was merged, closed otherwise.
-- A card that has been done for more than `ARCHIVE_AFTER_DAYS` days is archived: it leaves the board and stays in the
-- store, in search and in the analytics. Change the number in this script for a project that wants another time.
local ARCHIVE_AFTER_DAYS = 14

function on_card(card)
  moebius.set_label("in-progress", false)
  moebius.record_outcome(card.merged and "merged" or "closed")
  if moebius.now() - card.moved_at > ARCHIVE_AFTER_DAYS * 86400 then
    moebius.archive(card)
  end
end
