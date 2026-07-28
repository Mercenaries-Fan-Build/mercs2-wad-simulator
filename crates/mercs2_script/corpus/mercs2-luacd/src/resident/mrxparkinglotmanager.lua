import("MrxTutorialManager")
import("MrxUtil")
import("MrxGui")
kiParkingLotLimit = 8
kfTutorialTime = 6
kiBlipSize = 6

function Setup()
  eParkingLotTracker = Event.CreatePersistent(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    0,
    "d",
    "xo"
  }, _TrackVehicle)
  eParkingLotTriggered = Event.CreatePersistent(Event.ScriptEvent, {
    "parkingLotStart",
    function(tData)
      return true
    end
  }, _MoveVehicle)
  _tParkingLotCandidates = {}
end

function Cleanup()
  Event.Delete(eParkingLotTracker)
  eParkingLotTracker = nil
  Event.Delete(eParkingLotTriggered)
  eParkingLotTriggered = nil
  _UnmarkVehicle()
  _tParkingLotCandidates = nil
end

function MarkLastVehicle()
  _UnmarkVehicle()
  if _uNewParkingLotVeh then
    _MarkVehicle(_uNewParkingLotVeh)
    _uNewParkingLotVeh = nil
  end
end

function _TrackVehicle(uChar, uVehicle)
  print(" =-= PARKING LOT adding: ", uVehicle)
  if Object.IsAlive(uVehicle) then
    if Object.HasLabel(uVeh, "Boat") or Object.HasLabel(uVeh, "Emplacedweapon") then
      return
    end
    for i, v in ipairs(_tParkingLotCandidates) do
      if v == uVehicle then
        table.remove(_tParkingLotCandidates, i)
        break
      end
    end
    if #_tParkingLotCandidates == kiParkingLotLimit then
      table.remove(_tParkingLotCandidates)
    end
    table.insert(_tParkingLotCandidates, 1, uVehicle)
  end
end

function _MoveVehicle(tData)
  local uRefPoint = tData[1]
  local uNormalPoint = tData[2]
  local uHeliPoint = tData[3]
  if type(uRefPoint) == "userdata" then
    Debug.Printf("=-= num candidates: ", #_tParkingLotCandidates)
    local uVeh = _GetLastVehicle(uRefPoint, uHeliPoint)
    if uVeh then
      local uDstPoint
      if Object.HasLabel(uVeh, "helicopter") then
        uDstPoint = uHeliPoint
      else
        uDstPoint = uNormalPoint
      end
      MrxUtil.ClearVehiclesNearPoint(uDstPoint, uVeh)
      Debug.Printf("=-= MOVING VEHICLE ", uVeh, uDstPoint)
      Object.SetTransformToObject(uVeh, uDstPoint)
      _uNewParkingLotVeh = uVeh
    end
  end
  Debug.Printf("=-= Cleaning up remaing parking lot vehicles ", #_tParkingLotCandidates)
  for i, v in pairs(_tParkingLotCandidates) do
    Object.Remove(v)
    _tParkingLotCandidates[i] = nil
  end
end

function _GetLastVehicle(uRefPos, uHeliPos)
  for i, uVeh in ipairs(_tParkingLotCandidates) do
    if Object.IsAlive(uVeh) and (Object.GetDistanceFrom(uVeh, uRefPos, true) < 65 or Object.HasLabel(uVeh, "helicopter") and Object.GetDistanceFrom(uVeh, uHeliPos, true) < 15) and Vehicle.GetDriver(uVeh) == nil then
      return table.remove(_tParkingLotCandidates, i)
    end
  end
end

function _MarkVehicle(uGuid)
  Debug.Printf(" =-= _MarkVehicle ", uGuid)
  local sIcon = "HUD_PMC_Fiona"
  local icon_r, icon_g, icon_b = MrxUtil.GetPrimaryObjectiveRgb()
  _uWorldMarker = Marker.AddBlip(uGuid, sIcon, 32, icon_r, icon_g, icon_b, 255, 1.25)
  if Net.IsServer() then
    Net.SendEvent_AddMarkerObjective(uGuid, _uWorldMarker, icon_r, icon_g, icon_b, 1.25, MrxUtil.MarkerGetIndexByName_World(sIcon), 1, 16)
  end
  local sName = "parkinglotVeh " .. Sys.GuidToString(uGuid)
  Hud.Radar:AddObjective({
    sName = sName,
    uGuid = uGuid,
    nR = 255,
    nG = 255,
    nB = 255,
    nWidth = kiBlipSize,
    nHeight = kiBlipSize,
    sTexture = "MiniMap_Icon_Faction_PMC",
    bSticky = true
  })
  local iMaxSize = kiBlipSize * 1.2
  Hud.Radar:AnimateObjectiveSize({
    sName = sName,
    nMaxWidth = iMaxSize,
    nMaxHeight = iMaxSize,
    nSpeedWidth = 20,
    nSpeedHeight = 20,
    nDuration = 1.5
  })
  eMarkEnter = Event.Create(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    uGuid,
    "a",
    "ei"
  }, _UnmarkVehicle)
  eMarkDeath = Event.Create(Event.ObjectDeath, {uGuid}, _UnmarkVehicle)
  eMarkHibernation = Event.Create(Event.ObjectHibernation, {uGuid, "hibernated"}, _UnmarkVehicle)
  _uParkingLotVeh = uGuid
  _ShowTutorial1()
end

function _UnmarkVehicle()
  Debug.Printf(" =-= UnmarkVehicle ", _uParkingLotVeh)
  if type(_uParkingLotVeh) == "userdata" then
    Marker.Remove(_uWorldMarker)
    if Net.IsServer() then
      Net.SendEvent_RemoveMarkerObjective(_uWorldMarker)
    end
    _uWorldMarker = nil
    Hud.Radar:RemoveObjective({
      sName = "parkinglotVeh " .. Sys.GuidToString(_uParkingLotVeh)
    })
    Event.Delete(eMarkEnter)
    eMarkEnter = nil
    Event.Delete(eMarkDeath)
    eMarkDeath = nil
    Event.Delete(eMarkHibernation)
    eMarkHibernation = nil
    _uParkingLotVeh = nil
    _HideTutorial()
  end
end

function _ShowTutorial1()
  MrxTutorialManager.ShowMessage("[TUTORIAL.ParkingLot.First]", false, "parkingLot")
  eTutorial = Event.Create(Event.TimerRelative, {kfTutorialTime}, _ShowTutorial2)
end

function _ShowTutorial2()
  MrxTutorialManager.ShowMessage("[TUTORIAL.ParkingLot.Second]", false, "parkingLot")
  eTutorial = Event.Create(Event.TimerRelative, {kfTutorialTime}, _HideTutorial)
end

function _HideTutorial()
  MrxTutorialManager.HideMessage(false, "parkingLot")
  Event.Delete(eTutorial)
  eTutorial = nil
end
