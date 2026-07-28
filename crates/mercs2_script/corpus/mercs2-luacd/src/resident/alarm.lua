import("DangerousBuilding")
import("MrxTutorialManager")
tEvents = tEvents or {}
tLights = {}
NETEVENT_ALARMACTIVATE = 0
NETEVENT_ALARMDEACTIVATE = 1

function NetEventCallback(nEventType, tArgs)
  Debug.Printf("Alarm: NetEventCallback: nEventType:" .. tostring(nEventType))
  if nEventType == NETEVENT_ALARMACTIVATE then
    NetSafeAlarmActivated(tArgs[1])
  elseif nEventType == NETEVENT_ALARMDEACTIVATE then
    NetSafeAlarmDeactivated(tArgs[1])
  end
end

function OnActivate(uGuid, iArg)
  Debug.Printf("Alarm.OnActivate")
  tEvents[uGuid] = tEvents[uGuid] or {}
  tLights[uGuid] = false
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, function()
    bLightStart = Vehicle.SetParts(uGuid, "LightFront", false)
    Debug.Printf("LightFront activated " .. tostring(bLightStart))
    SetupActivationEvents(uGuid)
  end)
end

function SendPlayerJoinEventsAlarm(uGuid)
  if tLights[uGuid] then
    Net.SendCustomEvent("Alarm", NETEVENT_ALARMACTIVATE, {uGuid})
  end
end

function OnDeath(uGuid)
  OnDeactivate(uGuid)
end

function OnDeactivate(uGuid)
  Debug.Printf("Alarm.DeActivate")
  tEvents = tEvents or {}
  tEvents[uGuid] = tEvents[uGuid] or {}
  if tEvents[uGuid].uCheckEvent then
    Event.Delete(tEvents[uGuid].uCheckEvent)
    tEvents[uGuid].uCheckEvent = nil
  end
  if tEvents[uGuid].uActivate then
    Event.Delete(tEvents[uGuid].uActivate)
    tEvents[uGuid].uActivate = nil
  end
  if tEvents[uGuid].uDeactivate then
    Event.Delete(tEvents[uGuid].uDeactivate)
    tEvents[uGuid].uDeactivate = nil
  end
  tEvents[uGuid] = nil
end

function SetupActivationEvents(uGuid)
  Pg.AddContextAction(uGuid, "[ContextAction.UseAlarm]")
  tEvents[uGuid].uActivate = Event.Create(Event.ContextAction, {
    Player.GetAnyCharacter(),
    uGuid
  }, Junk.ToggleAlarm, {uGuid})
  if not tEvents[uGuid].uPlayerJoined then
    tEvents[uGuid].uPlayerJoined = Event.CreatePersistent(Event.ScriptEvent, {
      "mpPlayerJoin",
      function(tData)
        Debug.Printf("Alarm: SetupActivationEvents: Received Event")
        return Net.IsServer() and not Player.IsLocal(tData[1])
      end
    }, SendPlayerJoinEventsAlarm, {uGuid})
  end
end

function SetupDeactivationEvents(uGuid)
  Pg.AddContextAction(uGuid, "[ContextAction.UseAlarm]")
  tEvents[uGuid].uDeactivate = Event.Create(Event.ContextAction, {
    Player.GetAnyCharacter(),
    uGuid
  }, AlarmDeactivated, {uGuid})
end

function OnUse(uGuid, bEnabled)
  Debug.Printf("Alarm.OnUse " .. tostring(bEnabled))
  if bEnabled then
    AlarmActivated(uGuid)
  else
    AlarmDeactivated(uGuid)
  end
end

function MuteAlarm(uGuid)
  Sound.StopSound(uGuid, "fol_alarm_bldg_01")
end

function AlarmActivated(uGuid)
  tEvents[uGuid].uCheckEvent = Event.CreatePersistent(Event.TimerRelative, {8}, CheckAlarm, {uGuid})
  Net.SendCustomEvent("Alarm", NETEVENT_ALARMACTIVATE, {uGuid})
  tLights[uGuid] = true
  x, y, z = Object.GetPosition(uGuid)
  tBuildings = Pg.FastCollectBuildings(x, y, z, 100, "Occupied")
  DangerousBuilding.TurnOn(tBuildings, true, false, true)
  Sound.CueSound(uGuid, "fol_bldg_alarm_activate")
  Sound.CueSound(uGuid, "fol_alarm_bldg_01")
  bLight = Vehicle.SetParts(uGuid, "LightFront", true)
  bCtrl = Vehicle.SetParts(uGuid, "CtrlRotation", true)
  Debug.Printf("LightFront activated " .. tostring(bLight))
  Debug.Printf("CtrlRotation activated " .. tostring(bCtrl))
  Pg.RemoveContextAction(uGuid)
  SetupDeactivationEvents(uGuid)
  Event.Create(Event.TimerRelative, {60}, MuteAlarm, {uGuid})
  MrxTutorialManager.StartTutorial("Alarm")
end

function CheckAlarm(uGuid)
  local bStayOn = false
  x, y, z = Object.GetPosition(uGuid)
  tBuildings = Pg.FastCollectBuildings(x, y, z, 100, "Occupied")
  for i, building in pairs(tBuildings) do
    if Object.IsAlive(building) then
      bStayOn = true
      break
    end
  end
  if not bStayOn then
    AlarmDeactivated(uGuid)
    Event.Delete(tEvents[uGuid].uCheckEvent)
    tEvents[uGuid].uCheckEvent = nil
  end
end

function AlarmDeactivated(uGuid)
  if tEvents[uGuid] and tEvents[uGuid].uCheckEvent then
    Event.Delete(tEvents[uGuid].uCheckEvent)
  end
  Net.SendCustomEvent("Alarm", NETEVENT_ALARMDEACTIVATE, {uGuid})
  tLights[uGuid] = false
  Sound.StopSound(uGuid, "fol_alarm_bldg_01")
  Sound.CueSound(uGuid, "fol_bldg_alarm_activate")
  Vehicle.SetParts(uGuid, "LightFront", false)
  Vehicle.SetParts(uGuid, "CtrlRotation", false)
  Pg.RemoveContextAction(uGuid)
  SetupActivationEvents(uGuid)
  if tEvents[uGuid] and tEvents[uGuid].uDeactivate then
    Event.Delete(tEvents[uGuid].uDeactivate)
    tEvents[uGuid].uDeactivate = nil
  end
end

function NetSafeAlarmActivated(uGuid)
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, function()
    x, y, z = Object.GetPosition(uGuid)
    tBuildings = Pg.FastCollectBuildings(x, y, z, 100, "Occupied")
    DangerousBuilding.TurnOn(tBuildings, true, false, true)
    Sound.CueSound(uGuid, "fol_bldg_alarm_activate")
    Sound.CueSound(uGuid, "fol_alarm_bldg_01")
    bLight = Vehicle.SetParts(uGuid, "LightFront", true)
    bCtrl = Vehicle.SetParts(uGuid, "CtrlRotation", true)
    Debug.Printf("LightFront activated " .. tostring(bLight))
    Debug.Printf("CtrlRotation activated " .. tostring(bCtrl))
  end)
end

function NetSafeAlarmDeactivated(uGuid)
  Sound.StopSound(uGuid, "fol_alarm_bldg_01")
  Sound.CueSound(uGuid, "fol_bldg_alarm_activate")
  Vehicle.SetParts(uGuid, "LightFront", false)
  Vehicle.SetParts(uGuid, "CtrlRotation", false)
  if tEvents[uGuid] and tEvents[uGuid].uDeactivate then
    Event.Delete(tEvents[uGuid].uDeactivate)
    tEvents[uGuid].uDeactivate = nil
  end
end
