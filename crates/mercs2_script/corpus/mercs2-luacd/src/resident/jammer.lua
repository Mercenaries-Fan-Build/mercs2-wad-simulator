import("MrxSupport")
tEvents = tEvents or {}

function OnActivate(uGuid, iArg)
  tEvents[uGuid] = tEvents[uGuid] or {}
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupActivationEvents, {uGuid})
end

function OnDeactivate(uGuid)
  tEvents = tEvents or {}
  tEvents[uGuid] = tEvents[uGuid] or {}
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
  Object.PlayMaterialAnimation(uGuid, "global_gpsjammer_anim", false)
  Vehicle.SetParts(uGuid, "CtrlRotation", false)
  Pg.AddContextAction(uGuid, "[ContextAction.UseAlarm]")
  tEvents[uGuid].uActivate = Event.Create(Event.ContextAction, {
    Player.GetAnyCharacter(),
    uGuid
  }, OnUse, {uGuid})
end

function OnUse(uGuid)
  if tEvents[uGuid].uActivate then
    Event.Delete(tEvents[uGuid].uActivate)
    tEvents[uGuid].uActivate = nil
  end
  AlarmActivated(uGuid)
end

function AlarmActivated(uGuid)
  Object.PlayMaterialAnimation(uGuid, "global_gpsjammer_anim", true)
  Vehicle.SetParts(uGuid, "CtrlRotation", true)
  Pg.RemoveContextAction(uGuid)
  SetupDeactivationEvents(uGuid)
  MrxSupport.AddAntiAir(uGuid, "jammer")
end

function SetupDeactivationEvents(uGuid)
  Pg.AddContextAction(uGuid, "[ContextAction.UseAlarm]")
  tEvents[uGuid].uDeactivate = Event.Create(Event.ContextAction, {
    Player.GetAnyCharacter(),
    uGuid
  }, AlarmDeactivated, {uGuid})
end

function AlarmDeactivated(uGuid)
  Sound.StopSound(uGuid, "fol_alarm_bldg_01")
  Sound.CueSound(uGuid, "fol_bldg_alarm_activate")
  Object.PlayMaterialAnimation(uGuid, "global_gpsjammer_anim", false)
  Vehicle.SetParts(uGuid, "CtrlRotation", false)
  Pg.RemoveContextAction(uGuid)
  SetupActivationEvents(uGuid)
  if tEvents[uGuid].uDeactivate then
    Event.Delete(tEvents[uGuid].uDeactivate)
    tEvents[uGuid].uDeactivate = nil
  end
  MrxSupport.RemoveAntiAir(uGuid, "jammer")
end

function OnDeath(uGuid)
  MrxSupport.RemoveAntiAir(uGuid, "jammer")
end

function OnDeactivate(uGuid)
  MrxSupport.RemoveAntiAir(uGuid, "jammer")
end
