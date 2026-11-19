-- Ready to merge: the changes are in the inbox, one per repository the card works in, and on a forge their pull
-- requests were opened when the review approved them; a person merges them in the app or on the forge. If the forge's
-- checks of a pull request fail, or a default branch moved on and a change no longer merges cleanly, the card goes
-- back to In progress with the reason, which names the repository when the card works in several.

-- Which repository a note is about, when the card works in several: that of the change.
local function where(card, change)
  if #card.repositories > 1 then
    return "In " .. change.repository .. ": "
  end
  return ""
end

-- Why a change that no longer merges cleanly goes back to coding.
local function conflict_note(card, change)
  local base = change.base
  if #card.repositories > 1 then
    base = base .. " of " .. change.repository
  end
  return "The change no longer merges cleanly into " .. base .. ". These files conflict:\n" .. change.conflict
    .. "\nMerge " .. change.base .. " into your branch and resolve the conflicts."
end

function on_card(card)
  moebius.set_label("in-progress", false)
  for _, change in ipairs(card.changes) do
    if change.checks == "failure" then
      moebius.move("In progress", "The checks failed:\n" .. where(card, change)
        .. (change.failed_checks or change.check_output or ""))
      return
    elseif change.conflict ~= nil then
      moebius.move("In progress", conflict_note(card, change))
      return
    end
  end
  moebius.wait("waiting for a person to merge")
end
