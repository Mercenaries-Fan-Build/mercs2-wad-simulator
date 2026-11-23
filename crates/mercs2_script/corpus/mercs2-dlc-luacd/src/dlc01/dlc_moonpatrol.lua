local L0_1, L1_1, L2_1
import("MrxGuiBase", false)
tEvents = nil
uFilter = nil
NETEVENT_STARTEMITTERS = 0
NETEVENT_STOPEMITTERS = 1
nImpulse = 10
nPosition = 0.15

function L0_1(A0_2, A1_2)
  local L2_2, L3_2
  L2_2 = NETEVENT_STARTEMITTERS
  if A0_2 == L2_2 then
    StartEmitters(A1_2[1])
  else
    L2_2 = NETEVENT_STOPEMITTERS
    if A0_2 == L2_2 then
      StopEmitters(A1_2[1])
    end
  end
end

NetEventCallback = L0_1

function L0_1()
  local L0_2, L1_2
  tEvents = {}
end

Init = L0_1

function L0_1()
  local L0_2, L1_2
  tEvents = L0_2
end

Deinit = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2
  OnExit(0, A0_2)
end

OnActivate = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  OnDeactivate(A0_2)
end

OnDeath = L0_1

function L0_1(A0_2)
  local L1_2
  nImpulse = A0_2
end

SetImpulse = L0_1

function L0_1(A0_2)
  local L1_2
  nPosition = A0_2
end

SetPosition = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L3_2 = type(tEvents[A0_2])
  if L3_2 == "table" then
    L3_2 = pairs
    L4_2 = L2_2
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      Event.Delete(L7_2)
    end
  else
    Event.Delete(L2_2)
  end
  L3_2 = tEvents
  L3_2[A0_2] = nil
end

OnDeactivate = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = {}
  L5_2 = {}
  L5_2[1] = A0_2
  L5_2[2] = A1_2
  L5_2[3] = "d"
  L5_2[4] = "xo"
  L2_2.eExit = Event.Create(Event.ObjectInSeat, L5_2, OnExit)
  L3_2 = Event.Create
  L4_2 = Event.ScriptEvent
  L5_2 = {}
  L6_2 = "mpPlayerLeft"
  
  function L7_2(A0_3)
    local L1_3, L2_3
    L1_3 = A0_2 == A0_3[2]
    return L1_3
  end
  
  L5_2[1] = L6_2
  L5_2[2] = L7_2
  L7_2 = {}
  L7_2[1] = A0_2
  L7_2[2] = A1_2
  L2_2.eMPquit = L3_2(L4_2, L5_2, OnExit, L7_2)
  L3_2 = tEvents
  L3_2[A1_2] = L2_2
  ResetJump(A1_2)
end

OnEnter = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L3_2 = type(tEvents[A1_2])
  if L3_2 == "table" then
    L3_2 = pairs
    L4_2 = L2_2
    L3_2, L4_2, L5_2 = L3_2(L4_2)
    for L6_2, L7_2 in L3_2, L4_2, L5_2 do
      Event.Delete(L7_2)
    end
  end
  L3_2 = tEvents
  L6_2 = {}
  L6_2[1] = Player.GetAnyCharacter()
  L6_2[2] = A1_2
  L6_2[3] = "d"
  L6_2[4] = "ei"
  L3_2[A1_2] = Event.Create(Event.ObjectInSeat, L6_2, OnEnter)
  StopEmitters(A1_2)
  L3_2 = Net.IsActive()
  if L3_2 then
    L6_2 = {}
    L6_2[1] = A1_2
    Net.SendCustomEvent("moonpatrol", NETEVENT_STOPEMITTERS, L6_2)
  end
end

OnExit = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  if A0_2 then
    L1_2 = String.GetHash("global_particle_monstertruck_turbo")
    ObjectState.StartEmitter(A0_2, String.GetHash("hp_fx_exhaust_a"), L1_2)
    ObjectState.StartEmitter(A0_2, String.GetHash("hp_fx_exhaust_b"), L1_2)
    Sound.CueSound(A0_2, "dlcfx_truckBoost")
  end
end

StartEmitters = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2
  if A0_2 then
    L1_2 = String.GetHash("global_particle_monstertruck_turbo")
    ObjectState.StopEmitter(A0_2, String.GetHash("hp_fx_exhaust_a"), L1_2)
    ObjectState.StopEmitter(A0_2, String.GetHash("hp_fx_exhaust_b"), L1_2)
  end
end

StopEmitters = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L2_2 = Player.GetLocalPlayer
  L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2 = L2_2()
  L1_2 = MrxGuiBase.GetCurrentControlHolder(L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2)
  if L1_2 then
    ResetJump(A0_2)
    return
  end
  L1_2 = Object.GetMass(A0_2)
  Object.ApplyPointImpulse(A0_2, 0, (nImpulse * L1_2), (0.1 * L1_2), 0, 0, nPosition, true)
  StartEmitters(A0_2)
  L2_2 = Net.IsActive()
  if L2_2 then
    L5_2 = {}
    L5_2[1] = A0_2
    Net.SendCustomEvent("moonpatrol", NETEVENT_STARTEMITTERS, L5_2)
  end
  L2_2 = tEvents[A0_2]
  L5_2 = {}
  L5_2[1] = 1.5
  L7_2 = {}
  L7_2[1] = A0_2
  L2_2.eGrounded = Event.Create(Event.TimerRelative, L5_2, WaitForLanding, L7_2)
end

OnJump = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  StopEmitters(A0_2)
  L1_2 = Net.IsActive()
  if L1_2 then
    L4_2 = {}
    L4_2[1] = A0_2
    Net.SendCustomEvent("moonpatrol", NETEVENT_STOPEMITTERS, L4_2)
  end
  L1_2 = tEvents[A0_2]
  L4_2 = {}
  L4_2[1] = A0_2
  L4_2[2] = true
  L6_2 = {}
  L6_2[1] = A0_2
  L1_2.eGrounded = Event.Create(Event.ObjectIsGrounded, L4_2, ResetJump, L6_2)
end

WaitForLanding = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L1_2 = Vehicle.GetDriver(A0_2)
  L2_2 = Player.GetLocalCharacter()
  if L1_2 == L2_2 then
    L2_2 = tEvents[A0_2]
    L2_2.eGrounded = nil
    L5_2 = {}
    L5_2[1] = Object.IsPlayerControlled(L1_2)
    L5_2[2] = "rbutton"
    L5_2[3] = "press"
    L5_2[4] = true
    L7_2 = {}
    L7_2[1] = A0_2
    L2_2.eJump = Event.Create(Event.Button, L5_2, OnJump, L7_2)
  end
end

ResetJump = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  Object.CloseGate(A0_2)
  Event.Delete(tEvents[A0_2])
  L2_2 = tEvents
  L5_2 = {}
  L5_2[1] = uFilter
  L5_2[2] = A0_2
  L5_2[3] = "<"
  L5_2[4] = 10
  L5_2[5] = false
  L5_2[6] = false
  L7_2 = {}
  L7_2[1] = A0_2
  L2_2[A0_2] = Event.Create(Event.ObjectProximity, L5_2, Activated, L7_2)
end

Deactivated = L0_1
