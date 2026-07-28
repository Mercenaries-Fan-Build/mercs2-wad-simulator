tEvents = tEvents or {}

function OnActivate(uGuid, iArg)
  return
end

function OnActivateOld(uGuid, iArg)
  Pg.LoadAsset("player_mattias_bare_technoviking", "animation")
  bAssetLoaded = true
  tEvents[uGuid] = tEvents[uGuid] or {}
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, SetupActivationEvents, {uGuid})
end

function OnDeactivate(uGuid)
  if bAssetLoaded then
    Pg.UnloadAsset("player_mattias_bare_technoviking", "animation")
    bAssetLoaded = false
  end
  Event.Delete(oEvent)
  oEvent = nil
end

function SetupActivationEvents(uGuid)
  Pg.AddContextAction(uGuid, "Dance", false)
  oEvent = Event.Create(Event.ContextAction, {"hero", uGuid}, OnUse)
end

function OnUse(uCharacter, uGuid)
  if uCharacter == Player.GetPrimaryCharacter() then
    iPlayer = 1
  elseif uCharacter == Player.GetSecondaryCharacter() then
    iPlayer = 2
  end
  if Net.IsServer() then
    Net.SendCustomEvent("DanceRadio", NETEVENT_STARTDANCING, {iPlayer, uGuid})
  end
  Pg.RemoveContextAction(uGuid)
  Human.DisableWeapons(uCharacter)
  Human.PlayRawAnimation(uCharacter, "player_mattias_bare_technoviking", false, false, 0, false)
  Event.Create(Event.TimerRelative, {1}, Event.Create, {
    Event.HumanStateTransition,
    {
      uCharacter,
      "*",
      "*",
      "complete"
    },
    Finished,
    {uGuid, uCharacter}
  })
end

function Finished(uGuid, uCharacter)
  Human.EnableWeapons(uCharacter)
  SetupActivationEvents(uGuid)
end

NETEVENT_STARTDANCING = 0

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_STARTDANCING then
    if tArgs[1] == 1 then
      uGuid = Player.GetPrimaryCharacter()
    elseif tArgs[1] == 2 then
      uGuid = Player.GetSecondaryCharacter()
    end
    OnUse(uGuid, tArgs[2])
  end
end
