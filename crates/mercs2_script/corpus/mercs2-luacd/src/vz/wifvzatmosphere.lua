tBoundaryList = {
  rgn_atmo_GRstripmine = function(bEnter)
  end,
  ["rgn_atmo_GR Cave"] = function(bEnter)
    Graphics.Atmosphere.SetSky("afternoon")
  end,
  rgn_atmo_Caracas = function(bEnter)
    Graphics.Atmosphere.SetSky("Maracaibo")
  end,
  ["rgn_atmo_PMC Outpost"] = function(bEnter)
    Graphics.Atmosphere.SetSky("afternoon")
  end
}

function SetDefaultAtmosphere()
  Debug.Printf("Atmosphere: default VZ settings")
  Graphics.Atmosphere.SetSky("afternoon")
end

function Start()
  SetDefaultAtmosphere()
  SetupBoundaryEvents()
end

function SetupBoundaryEvents()
  if Sys.IsLoadingOrStreaming and (not Player.GetLocalCharacter() or Sys.IsLoadingOrStreaming()) then
    Event.Create(Event.TimerRelative, {2}, SetupBoundaryEvents)
    return
  end
  for sBoundaryName, fAtmosphere in pairs(tBoundaryList) do
    local uBoundaryGuid = Pg.GetGuidByName(sBoundaryName)
    SetupBoundaryEvent(uBoundaryGuid, sBoundaryName, "enter")
  end
end

function SetupBoundaryEvent(uBoundaryName, sBoundaryName, sAction)
  if uBoundaryName then
    Debug.Printf("Atmosphere: creating boundary check for boundary \"" .. tostring(sBoundaryName) .. "\" " .. sAction)
    Event.Create(Event.Boundary, {
      Player.GetLocalCharacter(),
      uBoundaryName,
      sAction,
      false
    }, CrossedBoundary, {sBoundaryName})
  end
end

function CrossedBoundary(sBoundaryName, uPlayerCharacter, uBoundaryGuid, sAction)
  if not (uPlayerCharacter and sBoundaryName) or not uBoundaryGuid then
    return
  end
  Debug.Printf("Atmosphere: " .. tostring(sBoundaryName) .. " " .. sAction)
  if sAction == "enter" then
    local fAtmosphereSettings = tBoundaryList[sBoundaryName]
    fAtmosphereSettings(true)
    SetupBoundaryEvent(uBoundaryGuid, sBoundaryName, "exit")
  elseif sAction == "exit" then
    local fAtmosphereSettings = tBoundaryList[sBoundaryName]
    fAtmosphereSettings(false)
    local bOutsideAllBoundaries = true
    if uPlayerCharacter ~= nil then
      for sBoundaryName, _ in pairs(tBoundaryList) do
        local uBoundaryGuid = Pg.GetGuidByName(sBoundaryName)
        if uBoundaryGuid and Object.InsideBoundary(uPlayerCharacter, uBoundaryGuid) then
          bOutsideAllBoundaries = false
          break
        end
      end
    end
    if bOutsideAllBoundaries then
      SetDefaultAtmosphere()
    end
    SetupBoundaryEvent(uBoundaryGuid, sBoundaryName, "enter")
  end
end
