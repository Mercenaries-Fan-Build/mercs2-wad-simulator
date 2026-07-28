function Init()
  uEvent = uEvent or {}
end

function Deinit()
  uEvent = nil
end

function OnActivate(uGuid, uOwner, nArg)
  uEvent[uGuid] = uEvent[uGuid] or {}
  Event.Create(Event.ObjectHibernation, {uGuid, "awake"}, Activate, {uGuid})
end

function Activate(uGuid)
  CreateEnterEvent(uGuid)
end

function CreateEnterEvent(uGuid)
  uEvent[uGuid] = uEvent[uGuid] or {}
  uEvent[uGuid].Enter = Event.Create(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    uGuid,
    "Gunner",
    "enter"
  }, Enter)
end

function CreateExitEvent(uGuid, uChar)
  uEvent[uGuid] = uEvent[uGuid] or {}
  uEvent[uGuid].Exit = Event.Create(Event.ObjectInSeat, {
    uChar,
    uGuid,
    "Gunner",
    "exit"
  }, Exit)
end

function Enter(uChar, uGuid)
  local uPlayer = Object.IsPlayerControlled(uChar)
  if not uPlayer or not Player.IsLocal(uPlayer) then
    Debug.Printf("A non-player triggered this event!")
    return
  end
  local uCamera = Player.GetCamera(uPlayer)
  if uCamera then
    Graphics.Camera.SetFocusParams(0, 0, 2, 2, 600, 4, 0)
  end
  CreateExitEvent(uGuid, uChar)
end

function Exit(uChar, uGuid)
  local uPlayer = Object.IsPlayerControlled(uChar)
  if not uPlayer or not Player.IsLocal(uPlayer) then
    Debug.Printf("A non-player triggered this event!")
    return
  end
  local uCamera = Player.GetCamera(uPlayer)
  if uCamera then
    Graphics.Camera.RestoreFocusParams(0, 0)
  end
  CreateEnterEvent(uGuid)
end

function OnDeactivate(uGuid, nArg)
  if uEvent[uGuid] then
    Event.Delete(uEvent[uGuid].Enter)
    Event.Delete(uEvent[uGuid].Exit)
    uEvent[uGuid] = nil
  end
end
