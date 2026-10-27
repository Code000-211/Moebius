-- In progress: one Coder run per round; when it succeeds, the change goes to review. After a review that asked for
-- changes, the next round gets its findings.
function on_card(card)
  local run = card.run
  if run ~= nil and (run.status == "starting" or run.status == "running") then
    moebius.wait("coding")
  elseif run ~= nil and run.purpose == "code" and run.status == "succeeded" then
    moebius.move("In review")
  elseif card.rounds >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  else
    local findings = nil
    if card.review ~= nil then
      findings = card.review.findings
    end
    moebius.start_run("Coder", "code", findings)
    if card.rounds == 0 then
      moebius.wait("coding")
    else
      moebius.wait("coding, round " .. (card.rounds + 1))
    end
  end
end
