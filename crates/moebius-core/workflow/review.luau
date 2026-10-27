-- In review: the change is published (on GitHub that opens or updates the pull request), then waits for its checks,
-- then must merge cleanly, then the Reviewer reads it and ends with a verdict. Failing checks, a conflict or findings
-- send the card back to In progress with a note saying why. Approved, or without a Reviewer, the change is offered.
-- A review a person stopped, too many reviews or the Reviewer's spent token budget put the card in the inbox as stuck;
-- while the daily token budget is spent no review starts.
local function conflict_note(change)
  return "The change no longer merges cleanly into " .. change.base .. ". These files conflict:\n"
    .. change.conflict .. "\nMerge " .. change.base .. " into your branch and resolve the conflicts."
end

-- Whether a limit keeps the Reviewer from starting now; the card is then stuck or waiting.
local function limited(card)
  local budget = moebius.agent_budget("Reviewer")
  if budget ~= nil and card.tokens >= budget then
    moebius.stuck("the token budget of " .. budget .. " is spent")
  elseif moebius.day_budget ~= nil and moebius.tokens_today() >= moebius.day_budget then
    moebius.wait("the daily token budget is spent")
  else
    return false
  end
  return true
end

function on_card(card)
  local run = card.run
  if run ~= nil and (run.status == "starting" or run.status == "running") then
    moebius.wait("reviewing")
    return
  end
  if card.stopped then
    moebius.stuck("stopped by a person")
    return
  end
  if run == nil or (run.purpose ~= "code" and run.purpose ~= "review") or (run.purpose == "code" and run.status ~= "succeeded") then
    moebius.move("In progress")
    return
  end

  moebius.publish_change()
  local change = moebius.run_checks()
  if change.checks == "pending" then
    moebius.wait("waiting for the checks")
  elseif change.checks == "failure" then
    moebius.move("In progress", "The checks failed:\n" .. (change.failed_checks or change.check_output or ""))
  elseif change.conflict ~= nil then
    moebius.move("In progress", conflict_note(change))
  elseif not moebius.has_agent("Reviewer") or (card.review ~= nil and card.review.verdict == "approve") then
    moebius.offer_change()
  elseif card.review ~= nil then
    moebius.move("In progress", card.review.findings)
  elseif run.purpose == "code" then
    if not limited(card) then
      moebius.start_run("Reviewer", "review")
      moebius.wait("reviewing")
    end
  elseif card.reviews >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  elseif not limited(card) then
    moebius.start_run("Reviewer", "review")
    moebius.wait("reviewing, try " .. (card.reviews + 1))
  end
end
