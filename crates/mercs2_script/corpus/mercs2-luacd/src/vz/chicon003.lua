inherit("MrxTaskContract")
import("MrxCinematic")
import("DangerousBuilding")
import("Outpost")
import("MrxSupportData")
import("MrxFactionManager")

function Activated(self)
  MrxTaskContract.Activated(self)
  nCompleted = 0
  bAlliesHateYou = false
  self:_CreateEvent(Event.ObjectHibernation, {
    Player.GetLocalCharacter(),
    "awake"
  }, Start, {self})
end

function Start(self)
  SetupObjectives(self)
  CaracasIsHell(self)
end

function SetupObjectives(self)
  CreateProxEvents(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi03-17",
    2,
    "Fiona-In-Mission-Contract-Chi03-26",
    {
      function()
        MrxSupportData.AddFreebie("CH_CruiseMissile")
        MrxSupportData.AddFreebie("CH_CruiseMissile")
        MrxSupportData.AddFreebie("CH_CruiseMissile")
        MrxSupportData.AddFreebie("CH_CruiseMissile")
      end
    },
    {
      mattias = "Mattias-In-Mission-Contract-Chi03-32",
      chris = "Chris-In-Mission-Contract-Chi03-34",
      jennifer = "Jennifer-In-Mission-Contract-Chi03-33"
    }
  })
  self:CreateChild({
    sName = "Verify",
    sModuleName = "MrxTaskObjectiveVerify",
    vTgtInclude = {
      "ChiCon003_HVT"
    },
    sDspShortDesc = "[ChiCon003.Objective.verify]",
    sFactionId = "Chi",
    tOnTargetDestroyed = {
      {
        MrxFactionManager.SetFactionReporting,
        {"All", false}
      }
    },
    tOnComplete = {
      {
        MrxFactionManager.SetFactionReporting,
        {"All", false}
      },
      {
        OneObjectiveDown,
        {self}
      },
      {
        AlliesHateYou,
        {self}
      }
    }
  })
  tBuildings = {
    "ChiCon003_Target01",
    "ChiCon003_Target02",
    "ChiCon003_Target04",
    "ChiCon003_Target05",
    "ChiCon003_Target06"
  }
  self:CreateChild({
    sName = "Destroy",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = tBuildings,
    sDspShortDesc = "[ChiCon003.Objective.destroy]",
    tOnPartComplete = {
      {
        AlliesHateYou,
        {self}
      }
    },
    tOnComplete = {
      {
        MrxSupportData.AddFreebie,
        {
          "ChiCon003_Artillery"
        }
      },
      {
        OneObjectiveDown,
        {self}
      }
    }
  })
  for i, building in pairs(tBuildings) do
    DangerousBuilding.TurnOn(building, false, false, true)
  end
end

function CreateProxEvents(self)
  self:_CreateEvent(Event.ObjectHibernation, {
    Pg.GetGuidByName("AllJob001_02_Outpost"),
    "awake"
  }, MrxVoSequence.Start, {
    {
      "Fiona-In-Mission-Contract-Chi03-08"
    }
  })
end

function OneObjectiveDown(self)
  nCompleted = nCompleted + 1
  if nCompleted == 2 then
    self:Complete()
  end
end

function BonusCancel(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi03-10"
  })
  MrxFactionManager.LockPursuit(Pg.GetGuidByName("Allied"), 3)
end

function DecrementTimer(self)
  oBonus._oTimer:AddTime(-600)
  if not bPlayedWarning then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-Chi03-11"
    })
    bPlayedWarning = true
  end
end

function AlliesHateYou(self)
  if bAlliesHateYou then
    return
  end
  bAlliesHateYou = true
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi03-18",
    {
      MrxFactionManager.ChangeRelation,
      {
        "All",
        "Pmc",
        -200
      }
    },
    {
      mattias = "Mattias-In-Mission-Contract-Chi03-19",
      chris = "Chris-In-Mission-Contract-Chi03-21",
      jennifer = "Jennifer-In-Mission-Contract-Chi03-20"
    },
    "Fiona-In-Mission-Contract-Chi03-22",
    {
      mattias = "Mattias-In-Mission-Contract-Chi03-23",
      chris = "Chris-In-Mission-Contract-Chi03-25",
      jennifer = "Jennifer-In-Mission-Contract-Chi03-24"
    }
  })
end

function CaracasIsHell(self)
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_caracas"), "warzone")
end

function SetBattlePathways(self)
  tAlliedAttackRoute01 = {
    "Road 0x000e76c0",
    "Road 0x000e76b9",
    "Road 0x000ee818",
    "Road 0x00110891",
    "Road 0x000ebd21",
    "Road 0x000ebd4e",
    "Road 0x000b0124",
    "Road 0x000b0125",
    "Road 0x000b0111",
    "Road 0x000b0123",
    "Road 0x000a1831",
    "Road 0x000a1992",
    "Road 0x0010b1bc",
    "Road 0x0010b3af",
    "Road 0x0010b215"
  }
  for i, road in pairs(tAlliedAttackRoute01) do
    Ai.SetLaneActive(Pg.GetGuidByName(road), 1, false)
  end
end

function _MissionComplete(self)
  self.Complete(self)
end

function Cleanup(self)
  MrxFactionManager.ClearPursuitLock()
  MrxSupportData.RemoveFreebie("CH_CruiseMissile")
  MrxTaskContract.Cleanup(self)
end
