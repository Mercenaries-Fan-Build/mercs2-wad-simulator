local L0_1, L1_1, L2_1
import("MrxPmc", false)
import("DLCComboMeter", false)
L0_1 = "pickup_crate_2"
tGlobalCashInstances = {}

function L1_1(A0_2, A1_2, A2_2)
  local L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L3_2 = tGlobalCashInstances[A0_2]
  if not L3_2 then
    L3_2 = {}
  end
  L3_2.Marker = Marker.Add(0, 0, 0, A0_2, 0, 255, 0, 0.5)
  L6_2 = {}
  L6_2[1] = 30
  L8_2 = {}
  L8_2[1] = A0_2
  L3_2.TimerEvent = Event.Create(Event.TimerRelative, L6_2, OnDeactivate, L8_2)
  L4_2 = Hud.Radar
  L6_2 = {}
  L6_2.sName = ("Cash" .. tostring(A0_2))
  L6_2.uGuid = A0_2
  L6_2.nR = 0
  L6_2.nG = 255
  L6_2.nB = 0
  L6_2.nWidth = A2_2
  L6_2.nHeight = A2_2
  L4_2.AddObjective(L4_2, L6_2)
  L6_2 = {}
  L6_2[1] = Player.GetAnyCharacter()
  L6_2[2] = A0_2
  L6_2[3] = "<"
  L6_2[4] = 7
  L6_2[5] = false
  L6_2[6] = true
  L8_2 = {}
  L8_2[1] = A2_2
  L8_2[2] = A0_2
  L3_2.PickupEvent = Event.Create(Event.ObjectProximity, L6_2, CashPickedUp, L8_2)
  L4_2 = tGlobalCashInstances
  L4_2[A0_2] = L3_2
end

OnActivate = L1_1

function L1_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L4_2 = {}
  L4_2[1] = A0_2
  Event.Post("CashPickup", L4_2)
  L2_2 = Object.GetPosition
  L3_2 = A1_2
  L2_2, L3_2, L4_2 = L2_2(L3_2)
  Pg.Spawn("global_particle_explosion_pickup_money", L2_2, L3_2, L4_2)
  OnDeactivate(A1_2)
end

CashPickedUp = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = tGlobalCashInstances[A0_2]
  if not L1_2 then
    return
  end
  L2_2 = L1_2.Marker
  if L2_2 then
    Marker.Remove(L1_2.Marker)
    L1_2.Marker = nil
  end
  L2_2 = Hud.Radar
  L4_2 = {}
  L4_2.sName = ("Cash" .. tostring(A0_2))
  L2_2.RemoveObjective(L2_2, L4_2)
  L2_2 = L1_2.TimerEvent
  if L2_2 then
    Event.Delete(L1_2.TimerEvent)
  end
  L2_2 = L1_2.ProximityEvent
  if L2_2 then
    Event.Delete(L1_2.ProximityEvent)
  end
  L2_2 = tGlobalCashInstances
  L2_2[A0_2] = nil
  Object.Remove(A0_2)
end

OnDeactivate = L1_1

function L1_1()
  local L0_2, L1_2, L2_2, L3_2, L4_2, L5_2, L6_2
  L0_2 = ipairs
  L1_2 = tGlobalCashInstances
  L0_2, L1_2, L2_2 = L0_2(L1_2)
  for L3_2, L4_2 in L0_2, L1_2, L2_2 do
    Event.Delete(tGlobalCashInstances.TimerEvent)
    Event.Delete(tGlobalCashInstances.ProximityEvent)
    Object.Delete(L4_2)
  end
end

Cleanup = L1_1
