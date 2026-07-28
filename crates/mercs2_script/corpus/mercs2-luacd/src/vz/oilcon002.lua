inherit("MrxTaskContract")
import("MrxSubtitle")
import("MrxLayerManager")
import("MrxOilCon002Delivery")
import("MrxSupportData")
import("MrxTimer")
import("MrxGuiPda")
import("MrxPlayer")
import("MrxTransit")
import("MrxTutorialManager")
import("MrxUtil")
import("DangerousBuilding")
import("MrxFuelAirBomb")
import("MrxPmc")
import("MrxCinematic")
import("MrxTaskObjectiveDestroy")
import("WifBios")
tBlipLocations = {
  "oilcon002_loc_postA",
  "oilcon002_loc_postB",
  "oilcon002_loc_postC"
}
NETEVENT_STARTHACK = 0
NETEVENT_STOPHACK = 1
NETEVENT_MOVE_LUCKY_LADY = 2

function NetEventCallback(nEventType, tArgs)
  if nEventType == NETEVENT_STARTHACK then
    NetSafeStartHack()
  elseif nEventType == NETEVENT_STOPHACK then
    StopHack(tBlipLocations)
  elseif nEventType == NETEVENT_MOVE_LUCKY_LADY then
    Object.SetTransformToObject(tArgs[1], Pg.GetGuidByName("OilCon002_HeliTele"))
  end
end

function Activated(self)
  WifBios.AddDossierEntry("BioEwan")
  uPlayer = Player.GetAnyCharacter()
  uPrimaryPlayer = Player.GetPrimaryCharacter()
  oPostLoc01 = Pg.GetGuidByName("oilcon002_loc_postA")
  oPostLoc02 = Pg.GetGuidByName("oilcon002_loc_postB")
  oPostLoc03 = Pg.GetGuidByName("oilcon002_loc_postC")
  tVanList = {
    "ArmoredTruck_OilCon002_Target01",
    "ArmoredTruck_OilCon002_Target02",
    "ArmoredTruck_OilCon002_Target03",
    "ArmoredTruck_OilCon002_Target04"
  }
  tPost01Lines = {
    "Fiona-In-Mission-Contract-Oil02-07",
    1.5,
    "Ewan-In-Mission-Contract-Oil02-104",
    1,
    {
      mattias = "Mattias-In-Mission-Contract-Oil02-105",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-106",
      chris = "Chris-In-Mission-Contract-Oil02-107"
    },
    "Fiona-In-Mission-Contract-Oil02-108"
  }
  tPost02Lines = {
    "Fiona-In-Mission-Contract-Oil02-42",
    1.5,
    "Ewan-None-Freeplay-Support-99",
    {
      mattias = "Mattias-In-Mission-Contract-Oil02-110",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-111",
      chris = "Chris-In-Mission-Contract-Oil02-112"
    },
    "Fiona-In-Mission-Contract-Oil02-113"
  }
  tPost03Lines = {
    "Fiona-In-Mission-Contract-Oil02-08",
    "Rubin-In-Mission-Contract-Oil02-29",
    "VZSoldier-In-Mission-Contract-Oil02-26",
    "Rubin-In-Mission-Contract-Oil02-27",
    "VZSoldier-In-Mission-Contract-Oil02-28",
    0.5,
    "Fiona-In-Mission-Contract-Oil02-25"
  }
  tDeliveryLines = {
    tPost01Lines,
    tPost02Lines,
    tPost03Lines
  }
  nTimeLimit = 15
  nTotalDeliveryLocs = table.getn(tBlipLocations)
  sDeliveryObjectiveText = "[OilCon002.Objectives.deliverPosts]"
  sDeadlineText = "[OilCon002.Display.deadline]"
  uRescueLoc = Pg.GetGuidByName("OilCon002_RescueSite")
  tFalse = {}
  tCompletedLoc = {}
  nPartsComplete = 0
  bCheckActive = true
  MrxTaskContract.Activated(self)
  local nCheckpointTime = self:_GetFlag("AllPostsPlaced")
  nRecoverStatus = self:_GetFlag("PartPostsPlaced")
  if nCheckpointTime then
    local tDeliveryLocations = MrxOilCon002Delivery.GetCurrentDropZones()
    Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ RECALLING CHECKPOINT NOW!!!!! " .. nCheckpointTime .. " SECONDS LEFT +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
    self:LoadVansLayer()
    nTimeLimit = nCheckpointTime
    oMissionTimer = MrxTimer:Create({
      nStartTime = nTimeLimit,
      iTray = 2,
      tDoneCallbacks = {
        {
          Cancel,
          {self}
        }
      }
    })
    oMissionTimer:Start()
    FreebieAdd(self, 1, false)
    MrxVoSequence.Start(tPost03Lines)
    Hud.ObjectiveTray:SetSlotToText({nSlot = 1, sText = sDeadlineText})
    for i, sLocation in ipairs(tDeliveryLocations) do
      local sCurrLoc = Pg.GetGuidByName(sLocation)
      local x, y, z = Object.GetPosition(sCurrLoc)
      eSpawnPosts = self:_CreateEvent(Event.ObjectProximity, {
        uPlayer,
        sCurrLoc,
        "<",
        150,
        false,
        true
      }, Pg.Spawn, {
        "Listening Post",
        x,
        y,
        z
      })
    end
  elseif nRecoverStatus then
    Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Testing data for recovery code " .. nRecoverStatus .. " +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
    self:RecoverPostsStatus()
  else
    Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ FOUND NO EXISTING CHECKPOINT!!!!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
    StartPostDeliveryObjective(self)
  end
  eSpawnMG = self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    Pg.GetGuidByName("OilCon002_ParkingStructure01(critical)"),
    "<",
    75,
    false,
    true
  }, function()
    Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Spawning garage machinegunner!!!!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
    uParkingStructureMG = Pg.Spawn("Emplaced MG (VZ)", 3038.45, -4.41, 1461.63, 90)
  end, {self})
  Vehicle.Usable(Pg.GetGuidByName("OilCon002_VZAirportHeli01"), false)
  Vehicle.Usable(Pg.GetGuidByName("OilCon002_VZAirportHeli02"), false)
  eVZPilotTakeoff = self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    Pg.GetGuidByName("OilCon002_VZHeliPilot"),
    "<",
    150,
    false,
    true
  }, function(self)
    uVZPilot = Pg.Spawn("VZ Officer", 3171, -3.63, 1501.2)
    eVZPilotGetIn = self:_CreateEvent(Event.ObjectIsReady, {uVZPilot}, function(self)
      Vehicle.Usable(Pg.GetGuidByName("OilCon002_VZAirportHeli01"), true)
      Ai.Goal({
        AIGuid = uVZPilot,
        Goal = "MoveTo",
        Target = Pg.GetGuidByName("OilCon002_VZAirportHeli01"),
        Haste = 0.7,
        Priority = "HiPri",
        Callback = Vehicle.Enter,
        CallbackData = {
          Pg.GetGuidByName("OilCon002_VZAirportHeli01"),
          uVZPilot,
          "d",
          false,
          false
        }
      })
      eVZHeliPatrol = self:_CreateEvent(Event.ObjectInSeat, {
        uVZPilot,
        Pg.GetGuidByName("OilCon002_VZAirportHeli01"),
        "d",
        "ei"
      }, function(self)
        Debug.Printf("********** VZ pilot should be attempting to take off now! **********")
        VZTakeoff = Ai.Goal({
          AIGuid = uVZPilot,
          Goal = "HeliTakeoff",
          Priority = "hiPri"
        })
        if VZTakeoff then
          Debug.Printf("********** VZ pilot should be attempting to patrol now! **********")
          Ai.Anchor({
            AIGuid = uActor,
            AnchorRadius = 200,
            AnchorGuid = Pg.GetGuidByName("OilCon002_VZAirportHeliPath")
          })
          Ai.Goal({
            AIGuid = uVZPilot,
            Goal = "PathMove",
            Target = Pg.GetGuidByName("OilCon002_VZAirportHeliPath"),
            Haste = 0.575,
            Mode = "Loop",
            Priority = "loPri"
          })
        end
      end, {self})
    end, {self})
  end, {self})
  eParkStructDeath = self:_CreateEvent(Event.ObjectDeath, {
    Pg.GetGuidByName("OilCon002_ParkingStructure01(critical)")
  }, GarageDeath, {self})
end

function StartPostDeliveryObjective(self)
  tCompleted = {
    oPostLoc01,
    oPostLoc02,
    oPostLoc03
  }
  tCompPosts = {}
  nReset = 0
  bReconfig = false
  MrxOilCon002Delivery.ResetDropZones()
  uMasterDeliveryObj = self:CreateChild({
    sName = "OilCon002_MasterDelivery",
    sModuleName = "MrxTaskObjective",
    sDspShortDesc = sDeliveryObjectiveText,
    nQuota = 3,
    bDspMsgCcl = false,
    tOnComplete = {
      {
        AllPostsCheck,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Fiona-Banter-Contract-Oil02-01",
      {
        mattias = "Mattias-Banter-Contract-Oil02-02",
        jennifer = "Jennifer-Banter-Contract-Oil02-03",
        chris = "Chris-Banter-Contract-Oil02-04"
      },
      2,
      "Fiona-In-Mission-Contract-Oil02-03",
      0.5,
      "Fiona-In-Mission-Contract-Oil02-06"
    },
    fOnInitialNotesComplete = function()
      MrxTutorialManager.ShowMessage("[OilCon002.Tutorial.pDAReminder]")
      TutorialRemove(self)
    end
  })
  if uMasterDeliveryObj then
    self:_CreateEvent(Event.TimerRelative, {34}, function(self)
      MrxVoSequence.Start({
        "Ewan-In-Mission-Contract-Oil02-96",
        {
          mattias = "Mattias-In-Mission-Contract-Oil02-100",
          jennifer = "Jennifer-In-Mission-Contract-Oil02-101",
          chris = "Chris-In-Mission-Contract-Oil02-102"
        },
        "Ewan-In-Mission-Contract-Oil02-103",
        5,
        "Fiona-In-Mission-Contract-Oil02-133",
        {
          FreebieAdd,
          {
            self,
            2,
            true
          }
        }
      })
    end, {self})
  end
  ExitReminder(self)
  tDeliveryLocations = MrxOilCon002Delivery.GetCurrentDropZones()
  uSubDelivery = {}
  for i, sLocation in ipairs(tDeliveryLocations) do
    Debug.Printf("********** " .. tostring(sLocation))
    AddPostDelivery(self, Pg.GetGuidByName(sLocation))
    uSubDelivery[sLocation] = self:CreateChild({
      sName = "OilCon002_SubDelivery" .. i,
      sModuleName = "MrxTaskObjectiveDeliver",
      sTgtLabelFilter = "Listening Post",
      sDspShortDesc = sDeliveryObjectiveText,
      bDspDescPda = false,
      vDestLoc = sLocation,
      nQuota = 1,
      fDist = 10,
      bUseDestRing = true,
      bDisplayHelpText = false,
      bXZOnly = false,
      bDspMsg = false,
      tOnComplete = {
        {
          SubDeliveryComplete,
          {self, sLocation}
        }
      }
    })
  end
end

function LoadVansLayer(self)
  Debug.Printf("Called LoadVansLayer")
  MrxLayerManager.Add("VZ_state_OilCon002_Objectives")
  MrxLayerManager.Add("VZ_state_OilCon002_Objectives02")
  eVanLayerReady = self:_CreateEvent(Event.TimerRelative, {2}, StartVansMoving, {self})
end

function StartVansMoving(self)
  if uSubDelivery then
    for i, objective in pairs(uSubDelivery) do
      objective:Cancel()
    end
  end
  Debug.Printf("Called LoadVansLayer")
  nTargNum = math.randi(table.getn(tVanList))
  sTargVan = tVanList[nTargNum]
  uTargVan = Pg.GetGuidByName(sTargVan)
  uBlipTarget = uTargVan
  uVZDriver = Vehicle.GetDriver(uTargVan)
  tVZShotgun = Vehicle.GetRiders(uTargVan, "p")
  uVZShotgun = tVZShotgun[1]
  eTargetVanDeath = self:_CreateEvent(Event.ObjectHealthLessThan, {uTargVan, 1}, TimeOut, {self})
  eVanReady = self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    uTargVan,
    "<",
    100,
    false,
    false
  }, HostageInVan, {self, uTargVan})
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+  The magic van is: " .. tostring(Object.GetName(uTargVan)))
  StartHack(self)
  self:_CreateEvent(Event.TimerRelative, {40}, StartHijackObjective, {self})
end

function HostageInVan(self, uTargVan)
  local x, y, z = Object.GetPosition(uTargVan)
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ SPAWNING THE HOSTAGE NOW!!!!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  uHostage = Pg.Spawn("OC Executive (OilCon002_Hostage)", x, y, z + 30)
  uHostSeat = Vehicle.GetSeatFromRider(uVZShotgun)
  Ai.SetRelation(GetGuidByName("OC"), uVZDriver, 0)
  Ai.SetRelation(GetGuidByName("OC"), uVZShotgun, 0)
  eHostageReady = self:_CreateEvent(Event.ObjectHibernation, {uHostage, "awake"}, function(self)
    Ai.SetState({
      AIGuid = uHostage,
      State = "Pacifist",
      Value = true
    })
    Ai.SetRelation(GetGuidByName("VZ"), uHostage, 0)
    bHostageIn = Vehicle.EnterBySeatGuid(uTargVan, uHostage, uHostSeat, true, false)
    if bHostageIn then
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ The hostage is in the kidnapper's van!!! " .. tostring(bHostageIn) .. "+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
      self:_CreateEvent(Event.TimerRelative, {1}, Object.Remove, {uVZShotgun})
    end
  end, {self})
  if uHostage then
    eHostageDeath = self:_CreateEvent(Event.ObjectDeath, {uHostage}, TimeOut, {self})
  end
end

function StartHijackObjective(self)
  StopHack(tBlipLocations)
  StopHack(tVanList)
  eHijackHint = self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    uTargVan,
    "<",
    10,
    false,
    false
  }, function(self)
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Oil02-135"
    })
  end, {self})
  oVanHijack = self:CreateChild({
    sName = "OilCon02_HijackTruck",
    sModuleName = "MrxTaskObjectiveEnterVehicle",
    sDspShortDesc = "[OilCon002.Objectives.hijack]",
    vTgtInclude = uTargVan,
    nQuota = 1,
    bUseAnySeat = false,
    tOnComplete = {
      {
        PlayerInVan,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      "Fiona-In-Mission-Contract-Oil02-19"
    }
  })
  eEwanHire = self:_CreateEvent(Event.TimerRelative, {8}, EwanInterlude, {self})
  eDriverCheck = self:_CreateEvent(Event.ObjectDeath, {uVZDriver}, function(self)
    Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ The VZ kidnapper has been killed!!!!")
    if Object.IsAlive(uHostage) then
      self.oVanHijack:Complete()
    end
  end, {self})
end

function StartRescueObjective(self)
  local uPlayerGuid = Vehicle.GetDriver(uTargVan)
  local nX, nY, nZ = Object.GetPosition(Pg.GetGuidByName("TransitHeli_Spawn"))
  local nF = Object.GetYaw(Pg.GetGuidByName("TransitHeli_Spawn"))
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil02-94"
  })
  self:CreateChild({
    sName = "OilCon02_HostageDelivery",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = uHostage,
    vDestLoc = Pg.GetGuidByName("OilCon002_RescueSite"),
    fDist = 15,
    bStop = true,
    bXZOnly = false,
    sDspShortDesc = "[OilCon002.Objectives.deliverHostage02]",
    uStartAttachedToPlayer = uPlayerGuid,
    tOnComplete = {
      {
        TransitStart,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  Ai.Anchor({
    AIGuid = uHostage,
    AnchorRadius = 50,
    AnchorGuid = uPlayerGuid
  })
  eSpawnLLTrans = self:_CreateEvent(Event.ObjectProximity, {
    uHostage,
    Pg.GetGuidByName("OilCon002_RescueSite"),
    "<",
    20,
    false,
    true
  }, function(self)
    uLuckyLady = Pg.Spawn("UH1 Transport (PMC) (Driver)", nX, nY, nZ, nF, false, true)
    uEwan = Vehicle.GetDriver(uLuckyLady)
    Ai.Goal({
      AIGuid = uEwan,
      Goal = "PathMove",
      Target = Pg.GetGuidByName("OilCon002_EwanHoldingPath"),
      Haste = 0.25,
      Priority = "LoPri"
    })
    ePilotEwanDeath = self:_CreateEvent(Event.ObjectDeath, {uEwan}, EwanDeath, {self})
    if uLuckyLady then
      Event.Delete(eSpawnEwan)
    end
  end, {self})
end

function PlayerInVan(self)
  nDist2hq = MrxUtil.GetDistanceToObject(uTargVan, 2517.5, -33.75, 1490.2, true)
  ClearTimer(self)
  eVanDeathCancel = Event.Delete(eTargetVanDeath)
  eTargetVanDeath = nil
  MrxVoSequence.Start({
    {
      "OilExec-In-Mission-Contract-Oil02-89",
      uHostage
    },
    0.25,
    {
      mattias = "Mattias-In-Mission-Contract-Oil02-90",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-91",
      chris = "Chris-In-Mission-Contract-Oil02-92"
    },
    0,
    {
      "OilExec-In-Mission-Contract-Oil02-93",
      uHostage
    },
    {
      StartRescueObjective,
      {self}
    }
  })
  eHostageFree = self:_CreateEvent(Event.ObjectInSeat, {
    uHostage,
    uTargVan,
    "p",
    "xo"
  }, Ai.SetRelation, {
    GetGuidByName("VZ"),
    uHostage,
    -100
  })
end

function Cleanup(self)
  MrxTaskContract.Cleanup(self)
  if oMissionTimer then
    ClearTimer(self)
  end
  if uLuckyLady then
    self:_CreateEvent(Event.ObjectHibernation, {uLuckyLady, "hibernated"}, Object.Remove, {uLuckyLady})
  end
  StopHack(tBlipLocations)
  StopHack(tVanList)
  RemovePosts(self)
  nTotal = 0
  MrxTransit.Reset()
  MrxSupportData.RemoveFreebie("OilCon002_OC")
  MrxSupportData.RemoveFreebie("OilCon002_EXT")
  MrxSupportData.RemoveFreebie("OilCon002_LightMG")
  MrxSupportData.RemoveFreebie("OC_ClusterBomb")
  MrxLayerManager.MarkForRemoval("Vz_State_OilCon002")
  MrxLayerManager.MarkForRemoval("VZ_state_OilCon002_Objectives")
  MrxLayerManager.MarkForRemoval("VZ_state_OilCon002_Objectives02")
  self:_CreateEvent(Event.TimerRelative, {0.75}, MrxLayerManager.Remove, {
    "VZ_state_PilCon002_Epilogue"
  })
  MrxSupportData.RemoveFreebie("OilCon002_Delivery")
end

function EwanInterlude(self)
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Calling the Ewan Interlude VO now!!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  MrxVoSequence.Start({
    {
      mattias = "Mattias-In-Mission-Contract-Oil02-114",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-115",
      chris = "Chris-In-Mission-Contract-Oil02-116"
    },
    "Ewan-In-Mission-Contract-Oil02-117",
    {
      mattias = "Mattias-In-Mission-Contract-Oil02-118",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-119",
      chris = "Chris-In-Mission-Contract-Oil02-120"
    },
    "Ewan-In-Mission-Contract-Oil02-121",
    {
      mattias = "Mattias-In-Mission-Contract-Oil02-122",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-123",
      chris = "Chris-In-Mission-Contract-Oil02-124"
    },
    "Ewan-In-Mission-Contract-Oil02-125"
  })
end

function TransitStart(self)
  local nPlayers = Player.GetCurrentPlayers()
  local pX, pY, pZ = Object.GetPosition(Pg.GetGuidByName("TransitHeli_Landing02"))
  uHostageRide = Vehicle.GetFromRider(uHostage)
  nCancelled = 1
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Hostage delivery objective complete!!!!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  if uHostageRide == nil then
    Ai.Goal({
      AIGuid = uHostage,
      Goal = "MoveTo",
      Target = Pg.GetGuidByName("OilCon002_Entrance2Helipad"),
      Haste = 0.1,
      Priority = "HiPri",
      Callback = Object.Remove,
      CallbackData = {uHostage}
    })
  else
    eHostageOut = self:_CreateEvent(Event.ObjectInSeat, {
      uHostage,
      uHostageRide,
      "p",
      "x"
    }, Ai.Goal, {
      AIGuid = uHostage,
      Goal = "MoveTo",
      Target = Pg.GetGuidByName("OilCon002_Entrance2Helipad"),
      Haste = 0.1,
      Priority = "HiPri",
      Callback = Object.Remove,
      CallbackData = {uHostage}
    })
  end
  if eHostageDeath then
    Event.Delete(eHostageDeath)
  end
  MrxVoSequence.Start({
    {
      "OilExec-In-Mission-Contract-Oil01-67",
      uHostage
    },
    0.75,
    {
      mattias = "Mattias-In-Mission-Contract-Oil02-126",
      jennifer = "Jennifer-In-Mission-Contract-Oil02-127",
      chris = "Chris-In-Mission-Contract-Oil02-128"
    },
    "Ewan-In-Mission-Contract-Oil02-129",
    {
      UPHeliLand,
      {self}
    }
  })
  eTransitHeliLanded = self:_CreateEvent(Event.ObjectProximity, {
    uLuckyLady,
    Pg.GetGuidByName("TransitHeli_Landing"),
    "<",
    4,
    true,
    true
  }, function(self)
    MrxVoSequence.Start({
      {
        "Ewan-In-Mission-Contract-Oil02-130",
        uEwan
      }
    })
  end, {self})
  eHeroIn = self:_CreateEvent(Event.ObjectInSeat, {
    uPrimaryPlayer,
    uLuckyLady,
    "a",
    "e"
  }, function(self)
    oTransitEnt.Complete(oTransitEnt)
  end, {self})
end

function PMCTransit(self)
  uChar = Player.GetPrimaryCharacter()
  uSeatTran = Vehicle.GetSeatFromRider(uChar)
  MrxVoSequence.Start({
    {
      "Ewan-In-Mission-Contract-Oil02-134",
      uEwan
    },
    0,
    {
      function(self)
        MrxTransit.SetSystemEnabled(true, false, false)
        MrxTransit.SetLocationEnabled(1, "Pmc", true)
        MrxTransit.OpenInterface(Player.GetLocalPlayer(), _TransitCallback)
      end,
      {self}
    }
  })
  oPMCTransit = self:CreateChild({
    sName = "OilCon02_PMCTransit_" .. nCancelled,
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = uPlayer,
    vDestLoc = "01_pmc_hq_lz_playerone",
    fDist = 10,
    sDspShortDesc = "[OilCon002.Objectives.transit]",
    bDspMsgCcl = false,
    fOnComplete = function()
      if eExitLady then
        Event.Delete(eExitLady)
      end
      if eHeroIn then
        Event.Delete(eHeroIn)
      end
      self:EpilogTrigger()
    end,
    fOnCancel = function()
      if bTransitReset then
        bTransitReset = false
      else
        self:Cancel()
      end
    end
  })
  eExitLady = self:_CreateEvent(Event.ObjectProximity, {
    uPrimaryPlayer,
    uLuckyLady,
    ">",
    20,
    false,
    true
  }, function(self)
    bTransitReset = true
    oPMCTransit.Cancel(oPMCTransit)
    nCancelled = nCancelled + 1
    self:ObjectifyLady()
    if eExitLady then
      Event.Delete(eExitLady)
    end
    if eReenterLady then
      Event.Delete(eReenterLady)
    end
  end, {self})
  self:LadyCheck()
end

function Epilogue(self)
  self:_CreateEvent(Event.TimerRelative, {0.5}, function(self)
    MrxTransit.SetSystemEnabled(true)
    MrxTransit.SetLocationEnabled(1, "Pmc")
    MrxTransit.SetLocationEnabled(2, "Oil")
    MrxVoSequence.Start({
      {
        mattias = "Mattias-In-Mission-Contract-Oil02-86",
        jennifer = "Jennifer-In-Mission-Contract-Oil02-84",
        chris = "Chris-In-Mission-Contract-Oil02-85"
      },
      {
        "Ewan-In-Mission-Contract-Oil02-87",
        uEpiEwan
      },
      {
        self.Complete,
        {self}
      }
    })
  end, {self})
end

function FinalDelivery(self)
  self:CreateChild({
    sName = "OilCon02_FinalDelivery",
    sModuleName = "MrxTaskObjectiveDeliver",
    vTgtInclude = uPlayer,
    bHumansFollow = true,
    vDestLoc = "OilCon002_FinalDeliveryLoc",
    fDist = 3,
    bStop = false,
    bXZOnly = false,
    sDspShortDesc = "[OilCon002.Objectives.finalDel]",
    uStartAttachedToPlayer = uPlayerGuid,
    tOnComplete = {
      {
        self.Complete,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    },
    vVoSeqOnAdd = {
      {
        mattias = "Mattias-In-Mission-Contract-Oil02-86",
        jennifer = "Jennifer-In-Mission-Contract-Oil02-84",
        chris = "Chris-In-Mission-Contract-Oil02-85"
      },
      0.75,
      "Ewan-In-Mission-Contract-Oil02-87"
    }
  })
end

function SubDeliveryComplete(self, sLocation)
  if sLocation == "oilcon002_loc_postA" then
    nPartsAdded = 100
  elseif sLocation == "oilcon002_loc_postB" then
    nPartsAdded = 10
  elseif sLocation == "oilcon002_loc_postC" then
    nPartsAdded = 1
  end
  Debug.Printf("SubDeliveryComplete: " .. tostring(sLocation))
  nCompletedDeliveries = nCompletedDeliveries or 0
  nCompletedDeliveries = nCompletedDeliveries + 1
  local nIndex
  for i, loc in pairs(tDeliveryLocations) do
    if sLocation == loc then
      nIndex = i
      break
    end
  end
  if nIndex then
    Debug.Printf("Removing " .. tostring(sLocation) .. " (" .. nIndex .. ")")
    MrxOilCon002Delivery.RemoveDropZone(nIndex)
  else
    Debug.Printf("Error: Could not find \"" .. tostring(sLocation) .. "\" in tDeliveryLocations")
  end
  uMasterDeliveryObj:CompletePart()
  if bRecovered then
    nTotal = nLocsDone
  else
    nTotal = nTotal or 0
  end
  nTotal = nTotal + 1
  if tDeliveryLines[nTotal] then
    self:_CreateEvent(Event.TimerRelative, {2}, function(self)
      MrxVoSequence.Start(tDeliveryLines[nTotal])
      tDeliveryLines[nTotal] = nil
    end, {self})
  end
  MrxSupportData.RemoveFreebie("OilCon002_Delivery")
  CheckPost(self, sLocation)
  nPartsComplete = nPartsComplete + nPartsAdded
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Code for current delivery locations completed: " .. nPartsComplete)
  local nAvailLocs = table.getn(tDeliveryLocations)
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Number of locations left: " .. nAvailLocs)
  if nTotal < 3 then
    self:PartPostsCheck()
  end
  if nTotal == 2 then
    self:StartTimer(nTimeLimit)
  end
  if bRecovered then
    bRecovered = false
  end
end

function NetSafeStartHack()
  for _, sLoc in pairs(tBlipLocations) do
    Debug.Printf("Blipping " .. tostring(sLoc))
    Minimap:AddObjectiveWithGuid(sLoc, Pg.GetGuidByName(sLoc), 0, 0, 0, 0, 255, 255, nil, nil, "HUD_objective_unknown", true, nil, nil, 6)
    Hud.Radar:AnimateObjectiveSize({
      sName = sLoc,
      nDuration = 60,
      nMaxWidth = 10,
      nMaxHeight = 10,
      nSpeedWidth = 25,
      nSpeedHeight = 25
    })
  end
end

function StartHack(self)
  bRndmFlash = true
  Debug.Printf("Called StartHack")
  NetSafeStartHack()
  eVanBlip01 = self:_CreateEvent(Event.TimerRelative, {2}, function(self)
    eVanBlip03 = self:_CreateEvent(Event.TimerRelative, {10}, function(self)
      self:HackVans(tVanList, 0.5, 4, 8, "HUD_objective_unknown")
      eVanBlip04 = self:_CreateEvent(Event.TimerRelative, {10}, function(self)
        StopHack(tVanList)
        bRndmFlash = true
        self:HackRand(tVanList, 0.8, 3, 9, "HUD_objective_unknown", 1, 0.9, 4)
      end, {self})
    end, {self})
  end, {self})
  if Net.IsServer() then
    Net.SendCustomEvent("OilCon002", NETEVENT_STARTHACK, {})
  end
end

function HackVans(self, tList, nMax, nRate, nSize, sBlip)
  for _, sLoc in pairs(tList) do
    Minimap:AddObjectiveWithGuid(sLoc, Pg.GetGuidByName(sLoc), 0, 0, 0, 255, 200, 0, nSize, nSize, sBlip, true, nil, nil, 6)
    Hud.Radar:AnimateObjectiveAlpha({
      sName = sLoc,
      nDuration = 30,
      nMinAlpha = 0.1,
      nMaxAlpha = nMax,
      nSpeed = nRate
    })
  end
end

function HackRand(self, tList, nMax, nRate, nSize, sBlip, nSwitch, nDie, nIter)
  local x, y, z = Object.GetPosition(Player.GetPrimaryCharacter())
  if tList == tRandBlip then
    tBlips = tRandBlip
  else
    tBlips = {}
    for _, sObj in pairs(tList) do
      Debug.Printf("Checking GUID for " .. tostring(sObj))
      sGuid = Pg.GetGuidByName(sObj)
      table.insert(tBlips, sGuid)
    end
  end
  nIter = nIter or 1
  nRand = math.randi(table.getn(tBlips))
  uBlip = tBlips[nRand]
  sName = tostring(uBlip) .. "_" .. nIter
  Debug.Printf("Blipping " .. sName .. " for " .. tostring(tList))
  Minimap:AddObjectiveWithGuid(sName, uBlip, 0, 0, 0, 255, 200, 0, nSize, nSize, sBlip, true, nil, nil, 6)
  Hud.Radar:AnimateObjectiveAlpha({
    sName = sName,
    nDuration = 30,
    nMinAlpha = 0.2,
    nMaxAlpha = nMax,
    nSpeed = nRate
  })
  eRandBlip = self:_CreateEvent(Event.TimerRelative, {nSwitch}, function(self)
    if bRndmFlash then
      self:HackRand(tList, nMax, nRate, nSize, sBlip, nSwitch, nDie, nIter + 10)
    end
  end, {self})
  eRandBlip02 = self:_CreateEvent(Event.TimerRelative, {nDie}, function(self)
    Minimap:DeleteObjective(sName)
  end, {self})
end

function StopHack(tHackTargs)
  if not tHackTargs then
    return
  end
  bRndmFlash = false
  for _, sLoc in pairs(tHackTargs) do
    Minimap:DeleteObjective(sLoc)
  end
  if Net.IsServer() then
    Net.SendCustomEvent("OilCon002", NETEVENT_STOPHACK, {})
  end
end

function StartTimer(self, nTime)
  if oMissionTimer then
    Debug.Printf("----------------------- Timer has already started.")
  else
    oMissionTimer = MrxTimer:Create({
      nStartTime = nTime * 60,
      iTray = 2,
      tDoneCallbacks = {
        {
          TimeOut,
          {self}
        }
      }
    })
    oMissionTimer:Start()
    Hud.ObjectiveTray:SetSlotToText({nSlot = 1, sText = sDeadlineText})
  end
end

function ClearTimer(self)
  if oMissionTimer then
    oMissionTimer:Stop()
    oMissionTimer = nil
  end
  Hud.ObjectiveTray:SetSlotToText({nSlot = 1, sText = " "})
end

function AggravateBuilding(self, sBuildingName)
  local uBuildingGuid = Pg.GetGuidByName(sBuildingName)
  DangerousBuilding.TurnOn(uBuildingGuid, false, true)
  Ai.TweakAttachedSpawners(uBuildingGuid, {
    SpawnerState = "on",
    SpawnList = "Spawnlist (VZ Balcony)"
  })
end

function EpilogTrigger(self)
  uEpiEwan = Pg.GetGuidByName("OilCon002_EpiEwan")
  uChar = Player.GetPrimaryCharacter()
  Object.SetTransformToObject(uLuckyLady, Pg.GetGuidByName("OilCon002_HeliTele"))
  if Net.IsServer() then
    Net.SendCustomEvent("OilCon002", NETEVENT_MOVE_LUCKY_LADY, {uLuckyLady}, true)
  end
  self:_CreateEvent(Event.TimerRelative, {0.75}, function(self)
    if ePilotEwanDeath then
      Event.Delete(ePilotEwanDeath)
    end
    Object.Remove(uEwan)
    Vehicle.Enter(uLuckyLady, uEpiEwan, "d", true, false)
    ePilotEwanDeath = self:_CreateEvent(Event.ObjectDeath, {uEpiEwan}, EwanDeath, {self})
    Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Attempting to put player in the Lucky Lady now!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
    bEpiPlayerIn = Vehicle.EnterBySeatGuid(uLuckyLady, uChar, uSeatTran, true, false)
    if bEpiPlayerIn then
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Player should be in the Lucky Lady now!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
      EwanLand(self, Pg.GetGuidByName("OilCon002_HeliTele"), uEpiEwan, false)
    end
  end, {self})
  if eReenterLady then
    Event.Delete(eReenterLady)
  end
  self:_CreateEvent(Event.TimerRelative, {2}, Epilogue, {self})
end

function AddPostDelivery(self, uAddPostLoc)
  Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_- Adding Listening Post *event* for location " .. tostring(uLocation) .. " for player " .. tostring(uPlayer))
  eAddPostDelivery01 = self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    uAddPostLoc,
    "<",
    25,
    false,
    false
  }, function(self)
    local x, y, z = Object.GetPosition(uAddPostLoc)
    local tPosts = Pg.GetObjectsInArea(x, y, z, 10, "Listening Post")
    local nPosts = table.getn(tPosts)
    if nPosts < 1 then
      MrxSupportData.AddFreebie("OilCon002_Delivery")
      Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_- Adding Listening Post delivery for location " .. tostring(uAddPostLoc))
      ResetPostDelivery(self, uAddPostLoc)
    end
  end, {self})
end

function ResetPostDelivery(self, uDelPostLoc)
  Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_- Adding Listening Post removal *event* for location " .. tostring(uDelPostLoc))
  eResetPostDelivery01 = self:_CreateEvent(Event.ObjectProximity, {
    Player.GetAllCharacters(),
    uDelPostLoc,
    ">",
    26,
    false,
    false
  }, function(self)
    MrxSupportData.RemoveFreebie("OilCon002_Delivery")
    Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_- Removing Listening Post delivery from location " .. tostring(uDelPostLoc))
    AddPostDelivery(self, uDelPostLoc)
  end, {self})
end

function AllPostsCheck(self)
  if oMissionTimer then
    nCheckpointTime = MrxTimer.GetTime(oMissionTimer)
  end
  bCheckActive = false
  self:_SetFlag("AllPostsPlaced", nCheckpointTime)
  _Checkpoint({
    "Starter_Oil0_Start1",
    "Starter_Oil0_Start2"
  })
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ ATTEMPTING TO SAVE CHECKPOINT NOW! TIME REMAINING AT RESTART: " .. nCheckpointTime .. " SECONDS +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  self:LoadVansLayer()
end

function PartPostsCheck(self)
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ ATTEMPTING TO SAVE CHECKPOINT NOW! Delivery locations completed status code: " .. nPartsComplete)
  self:_SetFlag("PartPostsPlaced", nPartsComplete)
  _Checkpoint({
    "Starter_Oil0_Start1",
    "Starter_Oil0_Start2"
  })
end

function RecoverPostsStatus(self)
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ ATTEMPTING TO LOAD CHECKPOINT NOW! Delivery locations completed status code: " .. tostring(nRecoverStatus))
  bRecovered = true
  nPartsComplete = nRecoverStatus
  if nRecoverStatus == 100 then
    tRecoverLocs = {
      "oilcon002_loc_postA"
    }
    tRemainingLocs = {
      "oilcon002_loc_postB",
      "oilcon002_loc_postC"
    }
  elseif nRecoverStatus == 10 then
    tRecoverLocs = {
      "oilcon002_loc_postB"
    }
    tRemainingLocs = {
      "oilcon002_loc_postA",
      "oilcon002_loc_postC"
    }
  elseif nRecoverStatus == 1 then
    tRecoverLocs = {
      "oilcon002_loc_postC"
    }
    tRemainingLocs = {
      "oilcon002_loc_postA",
      "oilcon002_loc_postB"
    }
  elseif nRecoverStatus == 110 then
    tRecoverLocs = {
      "oilcon002_loc_postA",
      "oilcon002_loc_postB"
    }
    tRemainingLocs = {
      "oilcon002_loc_postC"
    }
  elseif nRecoverStatus == 11 then
    tRecoverLocs = {
      "oilcon002_loc_postB",
      "oilcon002_loc_postC"
    }
    tRemainingLocs = {
      "oilcon002_loc_postA"
    }
  elseif nRecoverStatus == 101 then
    tRecoverLocs = {
      "oilcon002_loc_postA",
      "oilcon002_loc_postC"
    }
    tRemainingLocs = {
      "oilcon002_loc_postB"
    }
  end
  nLocsDone = table.getn(tRecoverLocs)
  for i, sLocation in ipairs(tRecoverLocs) do
    local sCurrLoc = Pg.GetGuidByName(sLocation)
    local x, y, z = Object.GetPosition(sCurrLoc)
    table.insert(tCompletedLoc, sLocation)
    eSpawnPosts = self:_CreateEvent(Event.ObjectProximity, {
      uPlayer,
      sCurrLoc,
      "<",
      150,
      false,
      true
    }, Pg.Spawn, {
      "Listening Post",
      x,
      y,
      z
    })
  end
  self:RecoveryObjective()
  if table.getn(tRemainingLocs) == 1 then
    self:StartTimer(nTimeLimit)
  end
end

function CheckPost(self, sLoc)
  local uLoc = Pg.GetGuidByName(sLoc)
  local x, y, z = Object.GetPosition(uLoc)
  local tPosts = Pg.GetObjectsInArea(x, y, z, 10, "Listening Post")
  local nPosts = table.getn(tPosts)
  local uPost = tPosts[1]
  table.insert(tCompletedLoc, sLoc)
  table.insert(tCompPosts, uPost)
  for i, loc in ipairs(tCompleted) do
    if uLoc == loc then
      tCompleted[loc] = uPost
      break
    end
  end
  Debug.Printf("-+-+-+-+-+-+-+-+ Checking Listening Post status at location " .. tostring(sLoc) .. ", " .. nPosts .. " active in the area.\n" .. tostring(tCompletedLoc))
  eCheckPostDist = self:_CreateEvent(Event.ObjectProximity, {
    uPost,
    uLoc,
    ">",
    10,
    false,
    false
  }, function(self)
    if bCheckActive then
      PostAlert(self, sLoc)
    end
  end, {self})
end

function RemovePosts(self)
  for i, sLoc in ipairs(tBlipLocations) do
    x, y, z = Object.GetPosition(Pg.GetGuidByName(sLoc))
    tPosts = Pg.GetObjectsInArea(x, y, z, 150, "Listening Post")
    for i, uPost in ipairs(tPosts) do
      self:_CreateEvent(Event.ObjectHibernation, {uPost, "hibernated"}, Object.Remove, {uPost})
    end
  end
end

function ResetMasterObj(self, sLoc)
  local sType = type(sLoc)
  local uLoc = Pg.GetGuidByName(sLoc)
  tResetLocs = {}
  for i, loc in ipairs(tCompletedLoc) do
    if sLoc == loc then
      table.insert(tResetLocs, sLoc)
      table.remove(tCompletedLoc, i)
      break
    end
  end
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Listening Post missing from location " .. tostring(sLoc) .. "!!!!! +-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
  if sLoc == "oilcon002_loc_postA" then
    nPartsMinus = 100
  elseif sLoc == "oilcon002_loc_postB" then
    nPartsMinus = 10
  elseif sLoc == "oilcon002_loc_postC" then
    nPartsMinus = 1
  end
  nPartsComplete = nPartsComplete - nPartsMinus
  nReset = nReset + 1
  nTotal = nTotal - 1
  nCompletedDeliveries = 0
  uMasterDeliveryObj:Cancel()
  bReset = true
  uMasterDeliveryObj = self:CreateChild({
    sName = "OilCon002_ResetMasterDelivery_" .. nReset,
    sModuleName = "MrxTaskObjective",
    sDspShortDesc = sDeliveryObjectiveText,
    nQuota = 3,
    bDspMsgAdd = false,
    tOnComplete = {
      {
        AllPostsCheck,
        {self}
      }
    }
  })
  MrxOilCon002Delivery.ResetDropZones()
  tDeliveryLocations = MrxOilCon002Delivery.GetCurrentDropZones()
  local nCompleted = table.getn(tCompletedLoc)
  bSetComps = uMasterDeliveryObj:Configure({nPartsCompleted = nCompleted})
  Debug.Printf("=-=-=-=-=-=-=-=-=-=- sLoc is a " .. tostring(sType))
  Debug.Printf("=-=-=-=-=-=-=-=-=-=- Attempting to reset delivery objectives for " .. tostring(tResetLocs))
  for i, sLocation in ipairs(tResetLocs) do
    local x, y, z = Object.GetPosition(Pg.GetGuidByName(sLocation))
    local tLocalHeroes = Pg.FastCollectHumans(x, y, z, 15, "Hero")
    local uLocation = Pg.GetGuidByName(sLocation)
    Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_- Adding Listening Post *event* for location " .. tostring(uLocation) .. " for player " .. tostring(uPlayer))
    eAddPostDeliveryReset = self:_CreateEvent(Event.ObjectProximity, {
      uPlayer,
      uLocation,
      "<",
      25,
      false,
      false
    }, function(self)
      MrxSupportData.AddFreebie("OilCon002_Delivery")
      Debug.Printf("-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_-\175-_- Adding Listening Post delivery for location " .. tostring(uLocation))
      ResetPostDelivery(self, uLocation)
    end, {self})
    uSubDelivery[sLocation] = self:CreateChild({
      sName = "OilCon002_SubDeliveryReset" .. i .. "_" .. nReset,
      sModuleName = "MrxTaskObjectiveDeliver",
      sTgtLabelFilter = "Listening Post",
      sDspShortDesc = sDeliveryObjectiveText,
      bDspDescPda = false,
      vDestLoc = sLocation,
      nQuota = 1,
      fDist = 10,
      bUseDestRing = true,
      bDisplayHelpText = false,
      bXZOnly = false,
      bDspMsg = false,
      tOnComplete = {
        {
          SubDeliveryComplete,
          {self, sLocation}
        }
      }
    })
  end
end

function RecoveryObjective(self)
  tCompleted = {
    oPostLoc01,
    oPostLoc02,
    oPostLoc03
  }
  tCompPosts = {}
  nReset = 0
  bReconfig = false
  MrxOilCon002Delivery.ResetDropZones()
  uMasterDeliveryObj = self:CreateChild({
    sName = "OilCon002_MasterDelivery",
    sModuleName = "MrxTaskObjective",
    sDspShortDesc = sDeliveryObjectiveText,
    nQuota = 3,
    bDspMsgCcl = false,
    tOnComplete = {
      {
        AllPostsCheck,
        {self}
      }
    }
  })
  uMasterDeliveryObj:Configure({nPartsCompleted = nLocsDone})
  Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ Number of locations completed set to " .. tostring(nLocsDone) .. "/3")
  FreebieAdd(self, 2, true)
  ExitReminder(self)
  if bRecovered then
    tDeliveryLocations = tRemainingLocs
  else
    tDeliveryLocations = MrxOilCon002Delivery.GetCurrentDropZones()
  end
  uSubDelivery = {}
  for i, sLocation in ipairs(tDeliveryLocations) do
    Debug.Printf("********** " .. tostring(sLocation))
    AddPostDelivery(self, Pg.GetGuidByName(sLocation))
    uSubDelivery[sLocation] = self:CreateChild({
      sName = "OilCon002_SubDelivery" .. i,
      sModuleName = "MrxTaskObjectiveDeliver",
      sTgtLabelFilter = "Listening Post",
      sDspShortDesc = sDeliveryObjectiveText,
      bDspDescPda = false,
      vDestLoc = sLocation,
      nQuota = 1,
      fDist = 10,
      bUseDestRing = true,
      bDisplayHelpText = false,
      bXZOnly = false,
      bDspMsg = false,
      tOnComplete = {
        {
          SubDeliveryComplete,
          {self, sLocation}
        }
      }
    })
  end
end

function PostAlert(self, sLoc)
  Debug.Printf("Blipping " .. tostring(sLoc))
  Minimap:AddObjectiveWithGuid(sLoc, Pg.GetGuidByName(sLoc), 0, 0, 0, 255, 0, 0, nil, nil, "HUD_objective_action", true, nil, nil, 5)
  Hud.Radar:AnimateObjectiveSize({
    sName = sLoc,
    nDuration = 60,
    nMaxWidth = 15,
    nMaxHeight = 15,
    nSpeedWidth = 45,
    nSpeedHeight = 45
  })
  eAlertTemp = self:_CreateEvent(Event.TimerRelative, {5}, function(self)
    Minimap:DeleteObjective(sLoc)
    ResetMasterObj(self, sLoc)
  end, {self, sLoc})
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Oil02-66"
  })
end

function HijackTutorial(self, sTank)
  local uTank = Pg.GetGuidByName(sTank)
  local bGunner = Vehicle.GetRiders(uTank, "g")
  eTankTutorial = self:_CreateEvent(Event.ObjectProximity, {
    uPlayer,
    uTank,
    "<",
    55,
    false,
    false
  }, function(self)
    if bGunner then
      MrxTutorialManager.ShowMessage("[OilCon002.Tutorial.hijackTutorial]")
      TutorialRemove(self)
      if eTankTutorial then
        Event.Delete(eTankTutorial)
      end
    end
  end, {self})
end

function ExitReminder(self)
  for i, sLocation in ipairs(tBlipLocations) do
    uLocation = Pg.GetGuidByName(sLocation)
    self:_CreateEvent(Event.ObjectProximity, {
      uPlayer,
      uLocation,
      "<",
      20,
      false,
      false
    }, function(self)
      if MrxPlayer.IsInVehicle("Vehicle") then
        MrxTutorialManager.ShowMessage("[OilCon002.Tutorial.supportReminder]")
        TutorialRemove(self)
      else
        MrxTutorialManager.ShowMessage("[OilCon002.Tutorial.supportTutorial]")
        TutorialRemove(self)
      end
    end, {self})
  end
end

function TutorialRemove(self)
  self:_CreateEvent(Event.TimerRelative, {5}, MrxTutorialManager.HideMessage, {self})
end

function GarageDeath(self)
  self:_SetCancelMessage("[OilCon002.Terms.Cancel02]")
  self:Cancel()
end

function TimeOut(self)
  self:_SetCancelMessage("[OilCon002.Terms.Cancel01]")
  self:Cancel()
end

function EwanDeath(self)
  self:_SetCancelMessage("[OilCon002.Terms.Cancel03]")
  self:Cancel()
end

function GetHostageSeat(self, uTruck)
  uDrSeat = Vehicle.GetSeatFromRider(uVZDriver)
  uPaSeat = Vehicle.GetSeatFromRider(uVZShotgun)
  tSeats = Object.GetAttachedObjects(uTruck)
  for i, uSeat in ipairs(tSeats) do
    bHuman = Object.HasLabel(uSeat, "Human")
    if uSeat ~= uDrSeat and uSeat ~= uPaSeat and bHuman == false then
      uHostSeat = uSeat
      Debug.Printf("-+-+-+-+-+-+-+-+-+-+-+-+-+-+-+ The hostage seat's GUID is " .. tostring(uSeat) .. "+-+-+-+-+-+-+-+-+-+-+-+-+-+-+-")
      break
    end
  end
end

function UPHeliLand(self)
  Ai.Goal({
    AIGuid = uEwan,
    Goal = "HeliLand",
    Target = Pg.GetGuidByName("TransitHeli_Landing"),
    Haste = 0.3,
    Priority = "HiPri",
    Force = true
  })
  EwanLand(self, Pg.GetGuidByName("TransitHeli_Landing"), uEwan, true)
  self:ObjectifyLady()
end

function ObjectifyLady(self)
  oTransitEnt = self:CreateChild({
    sName = "OilCon02_GetInHeli_" .. nCancelled,
    sModuleName = "MrxTaskObjective",
    vTgtInclude = uLuckyLady,
    sDspShortDesc = "[OilCon002.Objectives.enterLuckyLady]",
    bDspBlp = true,
    sDspBlpRdrIcon = "objective_action",
    sDspBlpWldIcon = "HUD_objective_action",
    tOnComplete = {
      {
        PMCTransit,
        {self}
      }
    },
    tOnCancel = {
      {
        self.Cancel,
        {self}
      }
    }
  })
  eHeroIn = self:_CreateEvent(Event.ObjectInSeat, {
    uPrimaryPlayer,
    uLuckyLady,
    "a",
    "e"
  }, function(self)
    oTransitEnt.Complete(oTransitEnt)
    if eHeroIn then
      Event.Delete(eHeroIn)
    end
  end, {self})
end

function LadyCheck(self)
  eReenterLady = self:_CreateEvent(Event.ObjectInSeat, {
    uPrimaryPlayer,
    uLuckyLady,
    "a",
    "e"
  }, function(self)
    MrxTransit.OpenInterface(Player.GetLocalPlayer(), _TransitCallback)
    self:LadyCheck()
  end, {self})
end

function EwanLand(self, uAnchor, uActor, bRotor)
  Ai.Anchor({
    AIGuid = uActor,
    AnchorRadius = 0,
    AnchorGuid = uAnchor
  })
  Ai.Goal({
    AIGuid = uActor,
    Goal = "Idle",
    MaintainRotorSpeed = bRotor,
    Priority = "medPri"
  })
end

function FreebieAdd(self, nQuant, bBomb)
  MrxSupportData.AddFreebie("OilCon002_OC", nQuant)
  MrxSupportData.AddFreebie("OilCon002_EXT", nQuant)
  MrxSupportData.AddFreebie("OilCon002_LightMG", nQuant)
  if bBomb then
    MrxSupportData.AddFreebie("OC_ClusterBomb", nQuant)
  end
end

function _TransitCallback(nSelectedIndex, bSuccess)
  if not bSuccess then
    return
  end
  MrxLayerManager.Add("VZ_state_PilCon002_Epilogue")
  MrxTransit.Transit(nSelectedIndex)
end

function LoadAssets(self)
  local tLayersToAdd = {
    "VZ_state_OilCon002_Pristine",
    "Vz_State_OilCon002"
  }
  MrxLayerManager.Add(tLayersToAdd, AssetsLoaded, {self})
end
