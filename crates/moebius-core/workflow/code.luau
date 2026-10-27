-- In progress: one Coder run per round; when it succeeds, the change is offered.
function on_card(card)
  local run = card.run
  if run == nil or run.purpose ~= "code" then
    moebius.start_run("Coder", "code")
    moebius.wait("coding")
  elseif run.status == "starting" or run.status == "running" then
    moebius.wait("coding")
  elseif run.status == "succeeded" then
    moebius.offer_change()
  elseif card.rounds >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  else
    moebius.start_run("Coder", "code")
    moebius.wait("coding, round " .. (card.rounds + 1))
  end
end
