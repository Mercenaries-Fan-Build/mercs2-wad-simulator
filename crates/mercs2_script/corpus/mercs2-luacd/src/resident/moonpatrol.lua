tEvents = nil
uFilter = nil
NETEVENT_STARTEMITTERS = 0
NETEVENT_STOPEMITTERS = 1

function NetEventCallback(eventId, tArgs)
  if eventId == NETEVENT_STARTEMITTERS then
    StartEmitters(tArgs[1])
  elseif eventId == NETEVENT_STOPEMITTERS then
    StopEmitters(tArgs[1])
  end
end

function Init()
  tEvents = {}
end

function Deinit()
  tEvents = nil
end

function OnActivate(uGuid, args)
  OnExit(0, uGuid)
end

function OnDeath(uGuid)
  OnDeactivate(uGuid)
end

function OnDeactivate(uGuid, args)
  local myEvents = tEvents[uGuid]
  if type(myEvents) == "table" then
    for i, e in pairs(myEvents) do
      Event.Delete(e)
    end
  else
    Event.Delete(myEvents)
  end
  tEvents[uGuid] = nil
end

function OnEnter(uDriver, uGuid)
  local myEvents = {}
  myEvents.eExit = Event.Create(Event.ObjectInSeat, {
    uDriver,
    uGuid,
    "d",
    "xo"
  }, OnExit)
  myEvents.eMPquit = Event.Create(Event.ScriptEvent, {
    "mpPlayerLeft",
    function(tData)
      return uDriver == tData[2]
    end
  }, OnExit, {uDriver, uGuid})
  tEvents[uGuid] = myEvents
  ResetJump(uGuid)
end

function OnExit(uDriver, uGuid)
  local myEvents = tEvents[uGuid]
  if type(myEvents) == "table" then
    for i, e in pairs(myEvents) do
      Event.Delete(e)
    end
  end
  tEvents[uGuid] = Event.Create(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    uGuid,
    "d",
    "ei"
  }, OnEnter)
  StopEmitters(uGuid)
  if Net.IsActive() then
    Net.SendCustomEvent("moonpatrol", NETEVENT_STOPEMITTERS, {uGuid})
  end
end

function StartEmitters(uGuid)
  if uGuid then
    local uEffectHash = String.GetHash("global_particle_monstertruck_turbo")
    ObjectState.StartEmitter(uGuid, String.GetHash("hp_fx_exhaust_a"), uEffectHash)
    ObjectState.StartEmitter(uGuid, String.GetHash("hp_fx_exhaust_b"), uEffectHash)
  end
end

function StopEmitters(uGuid)
  if uGuid then
    local uEffectHash = String.GetHash("global_particle_monstertruck_turbo")
    ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_exhaust_a"), uEffectHash)
    ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_exhaust_b"), uEffectHash)
  end
end

function OnJump(uGuid)
  local myMass = Object.GetMass(uGuid)
  Object.ApplyPointImpulse(uGuid, 0, 10 * myMass, 0.1 * myMass, 0, 0, 0.15, true)
  StartEmitters(uGuid)
  if Net.IsActive() then
    Net.SendCustomEvent("moonpatrol", NETEVENT_STARTEMITTERS, {uGuid})
  end
  local myEvents = tEvents[uGuid]
  myEvents.eGrounded = Event.Create(Event.TimerRelative, {1.5}, WaitForLanding, {uGuid})
end

function WaitForLanding(uGuid)
  StopEmitters(uGuid)
  if Net.IsActive() then
    Net.SendCustomEvent("moonpatrol", NETEVENT_STOPEMITTERS, {uGuid})
  end
  local myEvents = tEvents[uGuid]
  myEvents.eGrounded = Event.Create(Event.ObjectIsGrounded, {uGuid, true}, ResetJump, {uGuid})
end

function ResetJump(uGuid)
  local uDriver = Vehicle.GetDriver(uGuid)
  if uDriver == Player.GetLocalCharacter() then
    local myEvents = tEvents[uGuid]
    myEvents.eGrounded = nil
    myEvents.eJump = Event.Create(Event.Button, {
      Object.IsPlayerControlled(uDriver),
      "rtrigger",
      "press",
      true
    }, OnJump, {uGuid})
  end
end

function Deactivated(uGuid, tListOfObjects)
  Object.CloseGate(uGuid)
  Event.Delete(tEvents[uGuid])
  tEvents[uGuid] = Event.Create(Event.ObjectProximity, {
    uFilter,
    uGuid,
    "<",
    10,
    false,
    false
  }, Activated, {uGuid})
end
