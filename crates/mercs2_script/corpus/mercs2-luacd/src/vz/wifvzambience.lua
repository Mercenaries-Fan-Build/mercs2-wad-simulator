tBoundaryList = {}

function Start()
  for sBoundary, sAmbienceStream in pairs(tBoundaryList) do
    local uBoundaryGuid = Pg.GetGuidByName(sBoundary)
    SetupBoundaryEvent(uBoundaryGuid, sAmbienceStream, "any")
  end
end

function SetupBoundaryEvent(uBoundaryGuid, sAmbienceStream, sAction)
  if uBoundaryGuid then
    Debug.Printf("Ambience: creating boundary check for boundary \"" .. tostring(sAmbienceStream) .. "\"")
    Event.Create(Event.Boundary, {
      Player.GetLocalCharacter(),
      uBoundaryGuid,
      sAction,
      false
    }, CrossedBoundary, {sAmbienceStream})
  end
end

function CrossedBoundary(sAmbienceStream, uPlayerGuid, uBoundaryGuid, sAction)
  if not uBoundaryGuid or not sAmbienceStream then
    return
  end
  Debug.Printf("Ambience: " .. tostring(Object.GetName(uBoundaryGuid)) .. " " .. sAction .. " (" .. sAmbienceStream .. ")")
  if sAction == "enter" then
    Sound.CueAmbience(sAmbienceStream)
    SetupBoundaryEvent(uBoundaryGuid, sAmbienceStream, "exit")
  elseif sAction == "exit" then
    Sound.StopAmbience(sAmbienceStream)
    SetupBoundaryEvent(uBoundaryGuid, sAmbienceStream, "enter")
  end
end
