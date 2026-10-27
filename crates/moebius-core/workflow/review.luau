-- In review: the Reviewer reads the change and ends with a verdict. Approved, the change is offered; otherwise the
-- card goes back to In progress with the findings. Without a Reviewer, the change is offered at once.
function on_card(card)
  if not moebius.has_agent("Reviewer") then
    moebius.offer_change()
    return
  end
  local run = card.run
  if run == nil then
    moebius.move("In progress")
  elseif run.status == "starting" or run.status == "running" then
    moebius.wait("reviewing")
  elseif card.review ~= nil and card.review.verdict == "approve" then
    moebius.offer_change()
  elseif card.review ~= nil then
    moebius.move("In progress")
  elseif run.purpose == "code" and run.status == "succeeded" then
    moebius.start_run("Reviewer", "review")
    moebius.wait("reviewing")
  elseif run.purpose ~= "review" then
    moebius.move("In progress")
  elseif card.reviews >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  else
    moebius.start_run("Reviewer", "review")
    moebius.wait("reviewing, try " .. (card.reviews + 1))
  end
end
