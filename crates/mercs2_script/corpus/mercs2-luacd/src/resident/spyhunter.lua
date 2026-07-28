tEvents = nil
gbEnableBoostMeter = false
NETEVENT_STARTEMITTERS = 0
NETEVENT_STOPEMITTERS = 1
NETEVENT_STARTEMITTERSMOKE = 2
NETEVENT_STOPEMITTERSMOKE = 3

function Init()
  tEvents = {}
end

function Deinit()
  tEvents = nil
end

function OnActivate(uGuid, args)
  tEvents[uGuid] = {}
  local myEvents = tEvents[uGuid]
  myEvents.bReady = 1
  OnExit(0, uGuid)
  Debug.Printf("OnExit  another boat?^^^^^^^^^^^^^^^^ + player is ", tostring(uPlayer))
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
  local myEvents = tEvents[uGuid]
  Debug.Printf("^^^^^^^^^^^^^^^^ ON ENTER ", uGuid)
  myEvents.eExit = Event.Create(Event.ObjectInSeat, {
    uDriver,
    uGuid,
    "d",
    "a"
  }, OnExit)
  myEvents.eMPquit = Event.Create(Event.ScriptEvent, {
    "mpPlayerLeft",
    function(tData)
      return uDriver == tData[2]
    end
  }, OnExit, {uDriver, uGuid})
  tEvents[uGuid] = myEvents
  CoolCheck(uGuid)
  if gbEnableBoostMeter then
    DisplayBoost(uGuid)
  end
end

function CoolCheck(uGuid)
  local myEvents = tEvents[uGuid]
  if myEvents.bReady == 1 then
    ResetJump(uGuid)
  else
    myEvents.eCoolCheck = Event.Create(Event.TimerRelative, {0.5}, CoolCheck, {uGuid})
  end
end

function OnExit(uDriver, uGuid)
  local myEvents = tEvents[uGuid]
  Debug.Printf("^^^^^^^^^^^^^^^^ ON EXIT ", uGuid)
  if type(myEvents) == "table" then
    Event.Delete(myEvents.eJump)
    myEvents.eJump = nil
  end
  myEvents.eEnter = Event.Create(Event.ObjectInSeat, {
    Player.GetAnyCharacter(),
    uGuid,
    "d",
    "ei"
  }, OnEnter)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_orange_infinite")
  ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
  if Sound.SetVehicleEngineBoost then
    Sound.SetVehicleEngineBoost(uGuid, 0)
  end
end

function NetEventCallback(eventId, tArgs)
  if eventId == NETEVENT_STARTEMITTERS then
    NetSafeSetupBoost(tArgs[1])
  elseif eventId == NETEVENT_STOPEMITTERS then
    NetSafeStopBoost(tArgs[1])
  elseif eventId == NETEVENT_STARTEMITTERSMOKE then
    NetSafeSmokeStart(tArgs[1])
  elseif eventId == NETEVENT_STOPEMITTERSMOKE then
    NetSafeSmokeStop(tArgs[1])
  end
end

function GetDriverGuid(uGuid)
  local uDriver = Vehicle.GetDriver(uGuid)
  if uDriver == Player.GetLocalCharacter() then
    return Object.IsPlayerControlled(uDriver)
  end
  return nil
end

function DisplayBoost(uGuid)
  local sBarColor = "green"
  if nBoost <= 99 then
    sBarColor = "yellow"
  end
  if nBoost <= 30 then
    sBarColor = "red"
  end
  sHudText = [[

[GurCon003.Objectives.Boost]:[]] .. sBarColor .. "][bar" .. nBoost .. "]"
  uPlayer = GetDriverGuid(uGuid)
  Hud.ObjectiveTray:SetSlotToText({
    vPlayer = uPlayer,
    nSlot = 1,
    sText = sHudText
  })
end

function ResetJump(uGuid)
  local myEvents = tEvents[uGuid]
  if not myEvents.eJump then
    Debug.Printf("^^^^^^^^^^^^^^^^ RESET JUMP: ", uGuid)
    local uEffectHash = String.GetHash("global_particle_fire_jetengine_orange_infinite")
    ObjectState.StartEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
    uPlayer = GetDriverGuid(uGuid)
    if uPlayer == nil then
      OnExit(0, uGuid)
    else
      myEvents.eJump = Event.Create(Event.Button, {
        uPlayer,
        "lbutton",
        "press",
        true
      }, SetupBoost, {uGuid})
    end
  end
end

function NetSafeSmokeStop(uGuid)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_infinite")
  ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
end

function SetupBoost(uGuid)
  nJump = 0
  local myEvents = tEvents[uGuid]
  myEvents.bReady = 0
  myEvents.eJump = nil
  OnJump(uGuid)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_orange_infinite")
  ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_boost_infinite")
  ObjectState.StartEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
  if Sound.SetVehicleEngineBoost then
    Sound.SetVehicleEngineBoost(uGuid, 1)
  end
  if Net.IsActive() then
    Net.SendCustomEvent("spyhunter", NETEVENT_STARTEMITTERS, {uGuid})
  end
end

function NetSafeSetupBoost(uGuid)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_orange_infinite")
  ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_boost_infinite")
  ObjectState.StartEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
end

function OnJump(uGuid)
  if nJump <= 5 then
    local myMass = Object.GetMass(uGuid)
    if nJump == 0 then
      Object.ApplyPointImpulse(uGuid, 0, 0, 50000, 0, -0.5, -1.5, true)
    elseif nJump == 1 then
      Object.ApplyImpulse(uGuid, 0, 10000, 6 * myMass, true)
    elseif nJump == 2 then
      Object.ApplyImpulse(uGuid, 0, 20000, 8 * myMass, true)
    elseif nJump == 3 then
      Object.ApplyImpulse(uGuid, 0, -0.5 * myMass, 10 * myMass, true)
    elseif nJump == 4 then
      Object.ApplyImpulse(uGuid, 0, -0.5 * myMass, 6 * myMass, true)
    elseif nJump == 5 then
      Object.ApplyImpulse(uGuid, 0, -0.5 * myMass, 4 * myMass, true)
    end
    local myEvents = tEvents[uGuid]
    myEvents.eJumping = Event.Create(Event.TimerRelative, {0.8}, OnJump, {uGuid})
    nJump = nJump + 1
  else
    StopBoost(uGuid)
    Debug.Printf("Jump ended")
  end
end

function StopBoost(uGuid)
  Debug.Printf("^^^^^^^^^^^^^^^^ Stop Boost: ", uGuid)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_boost_infinite")
  ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
  if Sound.SetVehicleEngineBoost then
    Sound.SetVehicleEngineBoost(uGuid, 0)
  end
  if Net.IsActive() then
    Net.SendCustomEvent("spyhunter", NETEVENT_STOPEMITTERS, {uGuid})
  end
  CoolDown(uGuid)
end

function NetSafeStopBoost(uGuid)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_boost_infinite")
  ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
end

function NetSafeSmokeStart(uGuid)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_infinite")
  ObjectState.StartEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
end

function DontSmoke(uGuid)
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_infinite")
  ObjectState.StopEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
  if Net.IsActive() then
    Net.SendCustomEvent("spyhunter", NETEVENT_STOPEMITTERSMOKE, {uGuid})
  end
end

function IsCool(uGuid)
  local myEvents = tEvents[uGuid]
  myEvents.bReady = 1
  if GetDriverGuid(uGuid) then
    ResetJump(uGuid)
  end
end

function CoolDown(uGuid)
  local myEvents = tEvents[uGuid]
  myEvents.eSmokeStop = Event.Create(Event.TimerRelative, {5.5}, DontSmoke, {uGuid})
  myEvents.eReset = Event.Create(Event.TimerRelative, {8}, IsCool, {uGuid})
  local uEffectHash = String.GetHash("global_particle_fire_jetengine_infinite")
  ObjectState.StartEmitter(uGuid, String.GetHash("hp_fx_jetexhaust"), uEffectHash)
  if Net.IsActive() then
    Net.SendCustomEvent("spyhunter", NETEVENT_STARTEMITTERSMOKE, {uGuid})
  end
  Debug.Printf("^^^^^^^^^^^^^^^^ Cooldown: ", uGuid)
  if gbEnableBoostMeter then
    DisplayBoost(uGuid)
  end
end
