inherit("MrxTaskContract")
import("MrxCinematic")
import("DangerousBuilding")
import("Outpost")
import("MrxSupportData")
import("MrxFactionManager")
import("MrxLayerManager")

function Activated(self)
  MrxTaskContract.Activated(self)
  MrxLayerManager.Remove("state_car_city_act2ALL_staging")
  nCompleted = 0
  bHostile = false
  self:_CreateEvent(Event.ObjectHibernation, {
    Player.GetLocalCharacter(),
    "awake"
  }, Start, {self})
end

function Start(self)
  MrxSupportData.AddFreebie("AL_CruiseMissile")
  MrxSupportData.AddFreebie("AL_CruiseMissile")
  MrxSupportData.AddFreebie("AL_CruiseMissile")
  MrxSupportData.AddFreebie("AL_CruiseMissile")
  MrxSupportData.AddFreebie("Gunship")
  MrxSupportData.AddFreebie("Gunship")
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All03-18",
    {
      mattias = "Mattias-In-Mission-Contract-Chi03-32",
      chris = "Chris-In-Mission-Contract-Chi03-34",
      jennifer = "Jennifer-In-Mission-Contract-Chi03-33"
    }
  })
  SetupObjectives(self)
  CaracasIsHell(self)
  MrxLayerManager.Add("vz_state_allcon003_invasion")
end

function SetupObjectives(self)
  self:CreateChild({
    sName = "Verify",
    sModuleName = "MrxTaskObjectiveVerify",
    vTgtInclude = {
      "AllCon003_HVT"
    },
    sDspShortDesc = "[AllCon003.Objective.verify]",
    sFactionId = "All",
    tOnTargetDestroyed = {
      {
        GoHostile,
        {self}
      }
    },
    tOnComplete = {
      {
        OneObjectiveDown,
        {self}
      }
    }
  })
  self:CreateChild({
    sName = "DestroyBuildings",
    sModuleName = "MrxTaskObjectiveDestroy",
    vTgtInclude = {
      "AllCon003_A01",
      "AllCon003_A02",
      "AllCon003_A03",
      "AllCon003_A04"
    },
    sDspShortDesc = "[AllCon003.Objective.destroy]",
    tOnComplete = {
      {
        self.OneObjectiveDown,
        {self}
      }
    },
    tOnPartComplete = {
      {
        self.GoHostile,
        {self}
      }
    }
  })
end

function GoHostile(self)
  if bHostile then
    return
  end
  bHostile = true
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All03-13",
    {
      MrxFactionManager.ChangeRelation,
      {
        "Chi",
        "Pmc",
        -200
      }
    },
    {
      MrxFactionManager.SetFactionReporting,
      {"Chi", false}
    },
    {
      mattias = "Mattias-In-Mission-Contract-All03-14",
      chris = "Chris-In-Mission-Contract-All03-16",
      jennifer = "Jennifer-In-Mission-Contract-All03-15"
    },
    "Fiona-In-Mission-Contract-All03-17"
  })
end

function OneObjectiveDown(self)
  Ai.AddInfraction(Player.GetLocalCharacter(), Pg.GetGuidByName("China"), 100)
  nCompleted = nCompleted + 1
  if nCompleted == 2 then
    self:Complete()
  end
end

function NukeItFromOrbit(self)
  x, y, z = Object.GetPosition(Player.GetLocalCharacter())
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All03-20"
  })
  Airstrike.Flyby("Support Vehicle (Autogunship)", x - 50, z + 300, x, z, y + 100, 40)
end

function DecrementTimer(self)
  oBonus._oTimer:AddTime(-600)
  if not bPlayedWarning then
    MrxVoSequence.Start({
      "Fiona-In-Mission-Contract-All03-08"
    })
    bPlayedWarning = true
  end
end

function CaracasIsHell(self)
  Graphics.Atmosphere.ChangeLineRegionSetting(Pg.GetGuidByName("rgn_atmo_caracas"), "warzone")
end

function BonusCancel(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-All03-07"
  })
end

function Cleanup(self)
  MrxLayerManager.Add("state_car_city_act2ALL_staging")
  MrxSupportData.RemoveFreebie("AL_CruiseMissile")
  MrxTaskContract.Cleanup(self)
end
