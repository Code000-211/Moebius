-- In review: the change is published (on GitHub that opens or updates the pull request), then waits for its checks,
-- then must merge cleanly, then the Reviewer reads it along two axes, Standards and then Spec, each ending with
-- findings that cite evidence. Failing checks, a conflict or blocking findings send the card back to In progress with
-- a note saying why. Approved, or without a Reviewer, the change is offered, with the judgement calls for the person.
-- A review a person stopped, too many reviews or an agent's spent token budget put the card in the inbox as stuck;
-- while the daily token budget is spent no review starts. In a repository that works test first (`card.test_first`),
-- the change's reproduction test must also fail on the commit the change starts from and pass on the change before the
-- change is reviewed; a change that says it cannot be tested has the Spec axis judge why.
local function conflict_note(change)
  return "The change no longer merges cleanly into " .. change.base .. ". These files conflict:\n"
    .. change.conflict .. "\nMerge " .. change.base .. " into your branch and resolve the conflicts."
end

-- Whether a limit keeps the agent from starting now; the card is then stuck or waiting.
local function limited(card, agent)
  local budget = moebius.agent_budget(agent)
  if budget ~= nil and card.tokens >= budget then
    moebius.stuck("the token budget of " .. budget .. " is spent")
  elseif moebius.day_budget ~= nil and moebius.tokens_today() >= moebius.day_budget then
    moebius.wait("the daily token budget is spent")
  else
    return false
  end
  return true
end

-- The agent of an axis: one named for it ("Standards", "Spec"), else the Reviewer.
local function axis_agent(name)
  if moebius.has_agent(name) then
    return name
  end
  return "Reviewer"
end

-- Test first: how the change's reproduction test stands. "pending" while it runs; "untestable" and the reason the
-- change gives; "fails" and a note saying what did not hold; "holds" when it fails at the base and passes on the change.
local function reproduction()
  local test = moebius.reproduction()
  if test == nil then
    return "fails", "Test first: the change names no reproduction test. Write `.moebius/reproduce`: the command "
      .. "that runs the test on its first line and the files that hold it on the next ones, or `untestable: <why>`."
  end
  if test.untestable ~= nil then
    return "untestable", test.untestable
  end
  local base = moebius.run_test_at_base(test.command, test.files)
  local change = moebius.run_test(test.command)
  if base.status == "pending" or change.status == "pending" then
    return "pending"
  end
  local wrong = {}
  if base.status == "error" then
    table.insert(wrong, "It could not run on the commit the change starts from:\n" .. base.output)
  elseif base.status == "passed" then
    table.insert(wrong, "It passes on the commit the change starts from, so it does not reproduce the issue; it "
      .. "must fail there:\n" .. base.output)
  end
  if change.status == "error" then
    table.insert(wrong, "It could not run on the change:\n" .. change.output)
  elseif change.status == "failed" then
    table.insert(wrong, "It fails on the change; it must pass there:\n" .. change.output)
  end
  if #wrong == 0 then
    return "holds"
  end
  return "fails", "Test first: the reproduction test `" .. test.command .. "` does not hold.\n"
    .. table.concat(wrong, "\n")
end

-- After the checks: the reproduction test when the repository works test first, then the review.
local function review(card, run)
  local note = nil
  if card.test_first then
    local state, text = reproduction()
    if state == "pending" then
      moebius.wait("running the reproduction test")
      return
    elseif state == "fails" then
      moebius.move("In progress", text)
      return
    elseif state == "untestable" then
      note = "The change says it cannot be tested with a reproduction test: " .. text .. "\nJudge whether that "
        .. "holds; if it does not, ask for changes."
    end
  end
  if not moebius.has_agent("Reviewer") then
    moebius.offer_change()
  elseif card.review ~= nil then
    if card.review.verdict == "approve" then
      moebius.offer_change(card.review.judgements)
    else
      moebius.move("In progress", card.review.findings)
    end
  elseif run.purpose == "review-standards" and run.status == "succeeded" then
    local agent = axis_agent("Spec")
    if not limited(card, agent) then
      moebius.start_run(agent, "review-spec", note)
      moebius.wait("reviewing the spec")
    end
  elseif run.purpose ~= "code" and card.reviews >= moebius.round_limit then
    moebius.stuck("the round limit is reached")
  else
    local agent = axis_agent("Standards")
    if not limited(card, agent) then
      moebius.start_run(agent, "review-standards")
      if run.purpose == "code" then
        moebius.wait("reviewing the standards")
      else
        moebius.wait("reviewing, try " .. (card.reviews + 1))
      end
    end
  end
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
  local reviewing = run ~= nil and string.sub(run.purpose, 1, 6) == "review"
  if run == nil or (run.purpose ~= "code" and not reviewing) or (run.purpose == "code" and run.status ~= "succeeded") then
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
  else
    review(card, run)
  end
end
