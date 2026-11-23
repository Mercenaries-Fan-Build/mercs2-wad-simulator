local L0_1, L1_1, L2_1
import("DLCCon004a", false)
L0_1 = "pickup_crate_2"
tGlobalTimerInstances = {}

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = tGlobalTimerInstances[A0_2]
  if not L1_2 then
    L1_2 = {}
  end
  L1_2.Marker = Marker.Add(0, 0, 0, A0_2, 50, 0, 255, 1)
  L2_2 = Hud.Radar
  L4_2 = {}
  L4_2.sName = ("Timer" .. tostring(A0_2))
  L4_2.uGuid = A0_2
  L4_2.nR = 100
  L4_2.nG = 75
  L4_2.nB = 255
  L4_2.nWidth = 8
  L4_2.nHeight = 8
  L4_2.sTexture = "MiniMap_Icon_Clock_DLC"
  L2_2.AddObjective(L2_2, L4_2)
  L2_2 = Event.Create
  L3_2 = Event.ObjectProximity
  L4_2 = {}
  L4_2[1] = Player.GetAnyCharacter()
  L4_2[2] = A0_2
  L4_2[3] = "<"
  L4_2[4] = 7
  L4_2[5] = false
  L4_2[6] = true
  
  function L5_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3, L5_3, L6_3, L7_3
    L2_3 = {}
    L2_3[1] = 30
    Event.Post("TimeAdded", L2_3)
    L0_3 = Object.GetPosition
    L1_3 = A0_2
    L0_3, L1_3, L2_3 = L0_3(L1_3)
    Pg.Spawn("dlc_global_particle_explosion_pickup_timer", L0_3, L1_3, L2_3)
    OnDeactivate(A0_2)
  end
  
  L6_2 = {}
  L6_2[1] = iArg
  L1_2.PickupEvent = L2_2(L3_2, L4_2, L5_2, L6_2)
  L2_2 = tGlobalTimerInstances
  L2_2[A0_2] = L1_2
end

OnActivate = L1_1

function L1_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2
  L1_2 = tGlobalTimerInstances[A0_2]
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
  L4_2.sName = ("Timer" .. tostring(A0_2))
  L2_2.RemoveObjective(L2_2, L4_2)
  L2_2 = L1_2.ProximityEvent
  if L2_2 then
    Event.Delete(L1_2.ProximityEvent)
  end
  L2_2 = tGlobalTimerInstances
  L2_2[A0_2] = nil
  Object.Remove(A0_2)
end

OnDeactivate = L1_1
