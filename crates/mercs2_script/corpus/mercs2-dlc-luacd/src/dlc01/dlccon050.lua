local L0_1, L1_1, L2_1
inherit("MrxTaskContract", false)
import("MrxTaskObjectiveEnterVehicle", false)
import("MrxUtil", false)
import("MrxTimer", false)
import("MrxVoSequence", false)
import("DlcSpeedTimer", false)
import("DlcVehicleStrike", false)

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2, L10_2
  MrxTaskContract.Activated(A0_2)
  L1_2 = {}
  L2_2 = {}
  L2_2[1] = "monster truck"
  L2_2[2] = 80
  L2_2[3] = 10
  L3_2 = {}
  L3_2[1] = "escort"
  L3_2[2] = 50
  L3_2[3] = 20
  L4_2 = {}
  L4_2[1] = "Buggy (Hellfire)"
  L4_2[2] = 80
  L4_2[3] = 10
  L1_2[1] = L2_2
  L1_2[2] = L3_2
  L1_2[3] = L4_2
  L3_2 = Pg.SpawnFromCamera(L1_2[1][1], 20)
  A0_2.uCar = L3_2
  L5_2 = A0_2
  L4_2 = A0_2.CreateChild
  L6_2 = {}
  L6_2.sName = "Enter car"
  L6_2.sModuleName = "MrxTaskObjectiveEnterVehicle"
  L6_2.sDspShortDesc = "[MecCon001.Objectives.enterVehicle]"
  L6_2.vTgtInclude = L3_2
  L6_2.nQuota = 1
  
  function L7_2()
    local L0_3, L1_3, L2_3, L3_3, L4_3
    L0_3 = A0_2
    L0_3.CreateTimer(L0_3, 45)
    L0_3 = A0_2
    L0_3.tSpeedTimer = DlcSpeedTimer.Start(L3_2, L2_2[2], L2_2[3])
  end
  
  L6_2.fOnComplete = L7_2
  L4_2(L5_2, L6_2)
  L4_2 = A0_2._tEvents
  L7_2 = {}
  L7_2[1] = L3_2
  L9_2 = {}
  L9_2[1] = A0_2
  L4_2.eCarDeath = Event.Create(Event.ObjectDeath, L7_2, MrxTaskContract.Cancel, L9_2)
  TestStrike()
end

Activated = L0_1

function L0_1()
  local L0_2, L1_2, L2_2
  Pg.SpawnFromCamera("amx30", 60)
  Pg.SpawnFromCamera("amx30", 40)
  Pg.SpawnFromCamera("rtr")
  DlcVehicleStrike.CreateTankSupport()
  L0_2 = DlcVehicleStrike
  a = L0_2.Create(L0_2)
  L0_2 = a
  L0_2.AddStrike(L0_2)
  L0_2 = _G
  L0_2.a = a
end

TestStrike = L0_1

function L0_1(A0_2)
  local L1_2, L2_2, L3_2, L4_2
  L1_2 = A0_2.tSpeedTimer
  if L1_2 then
    DlcSpeedTimer.Cleanup(A0_2.tSpeedTimer)
    A0_2.tSpeedTimer = nil
  end
  L1_2 = A0_2.uTimer
  if L1_2 then
    L1_2 = A0_2.uTimer
    L1_2.Stop(L1_2)
  end
  L1_2 = Object.IsAlive(A0_2.uCar)
  if L1_2 then
    Vehicle.Exit(A0_2.uCar, Player.GetLocalCharacter(), true)
    Object.Remove(A0_2.uCar)
  end
  MrxTaskContract.Cleanup(A0_2)
end

Cleanup = L0_1

function L0_1(A0_2, A1_2)
  local L2_2, L3_2, L4_2, L5_2, L6_2, L7_2, L8_2, L9_2
  L2_2 = MrxTimer
  L4_2 = {}
  L4_2.nStartTime = A1_2
  L4_2.nStep = 1
  L4_2.bUseTenths = false
  L4_2.nWarning = 0
  L4_2.iTray = 1
  L4_2.bPlaySounds = false
  L5_2 = {}
  L6_2 = {}
  L8_2 = {}
  L8_2[1] = A0_2
  L6_2[1] = MrxTaskContract.Complete
  L6_2[2] = L8_2
  L5_2[1] = L6_2
  L4_2.tDoneCallbacks = L5_2
  A0_2.uTimer = L2_2.Create(L2_2, L4_2)
  L2_2 = A0_2.uTimer
  L2_2.Start(L2_2)
end

CreateTimer = L0_1
