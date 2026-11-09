-- Ready to merge: the change is in the inbox, and on a forge its pull request was opened when the review approved it; a
-- person merges it in the app or on the forge. If the forge's checks of the pull request fail, or the default branch
-- moved on and the change no longer merges cleanly, it goes back to In progress with the reason.
function on_card(card)
  moebius.set_label("in-progress", false)
  local change = card.change
  if change ~= nil and change.checks == "failure" then
    moebius.move("In progress", "The checks failed:\n" .. (change.failed_checks or change.check_output or ""))
  elseif change ~= nil and change.conflict ~= nil then
    moebius.move("In progress", "The change no longer merges cleanly into " .. change.base .. ". These files conflict:\n"
      .. change.conflict .. "\nMerge " .. change.base .. " into your branch and resolve the conflicts.")
  else
    moebius.wait("waiting for a person to merge")
  end
end
