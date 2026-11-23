local L0_1, L1_1, L2_1
import("MrxPmc", false)
import("MrxSubtitle", false)
import("MrxTankBuster", false)
import("MrxSupportData", false)
import("DlcVehicleStrike", false)
import("MrxTutorialManager", false)
L0_1 = tEvents
if not L0_1 then
  L0_1 = {}
end
tEvents = L0_1
L0_1 = tMarker
if not L0_1 then
  L0_1 = {}
end
tMarker = L0_1
L0_1 = tFocus
if not L0_1 then
  L0_1 = {}
end
tFocus = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2, L13_2
  L2_2 = tEvents
  L3_2 = tEvents[A0_2]
  if not L3_2 then
    L3_2 = {}
  end
  L2_2[A0_2] = L3_2
  L2_2 = tMarker
  L3_2 = tMarker[A0_2]
  if not L3_2 then
    L3_2 = {}
  end
  L2_2[A0_2] = L3_2
  L2_2 = tFocus
  L3_2 = tFocus[A0_2]
  if not L3_2 then
    L3_2 = {}
  end
  L2_2[A0_2] = L3_2
  L2_2 = Event.Create
  L3_2 = Event.ObjectHibernation
  L4_2 = {}
  L4_2[1] = A0_2
  L4_2[2] = "awake"
  
  function L5_2()
    local L0_3, L1_3
    SetupActivationEvents(A0_2)
  end
  
  L2_2(L3_2, L4_2, L5_2)
  L2_2 = tMarker[A0_2]
  L2_2.uMarker = Marker.AddBlip(A0_2, "pickup_munitions", 28, 51, 204, 153, 180, 0.75, 100, 150)
end

OnActivate = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  OnDeactivate(A0_2)
end

OnDeath = L0_1

function L0_1(A0_2)
  local L1_2, L2_2
  L1_2 = tEvents
  if L1_2 then
    L1_2 = tEvents[A0_2]
    if L1_2 then
      L1_2 = tEvents[A0_2].uCheckEvent
      if L1_2 then
        Event.Delete(tEvents[A0_2].uCheckEvent)
        L1_2 = tEvents[A0_2]
        L1_2.uCheckEvent = nil
      end
      L1_2 = tEvents[A0_2].uActivate
      if L1_2 then
        Event.Delete(tEvents[A0_2].uActivate)
        L1_2 = tEvents[A0_2]
        L1_2.uActivate = nil
      end
      L1_2 = tEvents[A0_2].uDeactivate
      if L1_2 then
        Event.Delete(tEvents[A0_2].uDeactivate)
        L1_2 = tEvents[A0_2]
        L1_2.uDeactivate = nil
      end
      L1_2 = tEvents[A0_2].uHideTutorial
      if L1_2 then
        Event.Delete(tEvents[A0_2].uHideTutorial)
        L1_2 = tEvents[A0_2]
        L1_2.uHideTutorial = nil
      end
      L1_2 = tEvents
      L1_2[A0_2] = nil
    end
  end
  L1_2 = tMarker
  if L1_2 then
    L1_2 = tMarker[A0_2]
    if L1_2 then
      L1_2 = tMarker[A0_2].uMarker
      if L1_2 then
        Marker.Remove(tMarker[A0_2].uMarker)
      end
    end
  end
end

OnDeactivate = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = tEvents[A0_2]
  L4_2 = {}
  L4_2[1] = Player.GetAnyCharacter()
  L4_2[2] = A0_2
  L4_2[3] = "<"
  L4_2[4] = 8
  L4_2[5] = false
  L4_2[6] = false
  L6_2 = {}
  L6_2[1] = A0_2
  L1_2.uActivate = Event.Create(Event.ObjectProximity, L4_2, CallStrike, L6_2)
end

SetupActivationEvents = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  L1_2 = Object.GetPosition
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  L4_2 = FindPlayer(A0_2)
  if L4_2 then
  else
    L4_2 = Player.GetPrimaryPlayer()
  end
  MrxSupportData.AddFreebie("FreeTankBuster")
  Pg.Spawn("global_particle_explosion_pickup_rocket", L1_2, L2_2, L3_2, 0)
  Object.FadeOut(A0_2, 0.075, true)
end

CallStrike = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2, L12_2
  L1_2 = Object.GetPosition
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  L7_2 = L3_2
  L4_2 = Pg.FastCollectHumans(L1_2, L2_2, L7_2, 20)
  L5_2 = ipairs
  L6_2 = L4_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = Object.IsPlayerControlled(L9_2)
    if L10_2 then
      return L10_2
    else
      L10_2 = FindPlayerVehicle(A0_2)
      return L10_2
    end
  end
end

FindPlayer = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2, L11_2
  L1_2 = Object.GetPosition
  L2_2 = A0_2
  L1_2, L2_2, L3_2 = L1_2(L2_2)
  L7_2 = L3_2
  L4_2 = Pg.FastCollectGroundVehicles(L1_2, L2_2, L7_2, 25)
  L5_2 = ipairs
  L6_2 = L4_2
  L5_2, L6_2, L7_2 = L5_2(L6_2)
  for L8_2, L9_2 in L5_2, L6_2, L7_2 do
    L10_2 = Object.IsPlayerControlled(L9_2)
    if L10_2 then
      return L10_2
    else
    end
  end
end

FindPlayerVehicle = L0_1
