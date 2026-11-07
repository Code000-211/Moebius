-- In progress: one Coder run per round; when it succeeds, the change goes to review. A card sent back by the review,
-- the checks, a conflict or a person carries a note saying why, which the next round gets until a round succeeds. A card
-- whose run a person stopped, which reached the round limit or which spent the Coder's token budget goes to the inbox
-- as stuck; a person retries it. While the daily token budget is spent no round starts.
function on_card(card)
  local run = card.run
  local budget = moebius.agent_budget("Coder")
  if run ~= nil and (run.status == "starting" or run.status == "running") then
    moebius.wait("coding")
  elseif run ~= nil and run.purpose == "code" and run.status == "succeeded" and card.note == nil then
    moebius.move("In review")
  elseif card.stopped then
    moebius.stuck("stopped by a person")
  elseif card.rounds >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  elseif budget ~= nil and card.tokens >= budget then
    moebius.stuck("the token budget of " .. budget .. " is spent")
  elseif moebius.day_budget ~= nil and moebius.tokens_today() >= moebius.day_budget then
    moebius.wait("the daily token budget is spent")
  else
    moebius.start_run("Coder", "code", card.note)
    if card.rounds == 0 then
      moebius.wait("coding")
    else
      moebius.wait("coding, round " .. (card.rounds + 1))
    end
  end
end
