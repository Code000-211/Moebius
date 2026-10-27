-- In progress: one Coder run per round; when it succeeds, the change goes to review. A card sent back by the review,
-- the checks or a conflict carries a note saying why, which the next round gets until a round succeeds.
function on_card(card)
  local run = card.run
  if run ~= nil and (run.status == "starting" or run.status == "running") then
    moebius.wait("coding")
  elseif run ~= nil and run.purpose == "code" and run.status == "succeeded" and card.note == nil then
    moebius.move("In review")
  elseif card.rounds >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  else
    moebius.start_run("Coder", "code", card.note)
    if card.rounds == 0 then
      moebius.wait("coding")
    else
      moebius.wait("coding, round " .. (card.rounds + 1))
    end
  end
end
