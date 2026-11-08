-- Done: every judgement made for the card learns how it ended: merged when its change was merged, closed otherwise.
function on_card(card)
  moebius.record_outcome(card.merged and "merged" or "closed")
end
