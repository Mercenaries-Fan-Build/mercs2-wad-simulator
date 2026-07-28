inherit("MrxTaskContractOutpost")

function GetOutpostConfig()
  return {
    sOutpostBldg = "ChiJob001_02_Outpost",
    tCapturePts = {
      "ChiJob001_02_CapturePt2"
    },
    sStagingLayer = "Vz_State_ChiJob001_02_Staging",
    sStagingTgLayer = "Vz_State_ChiCon051_Tg",
    sPristineLayer = "Vz_State_ChiJob001_02_Pristine",
    sDefenseLayer = "Vz_State_ChiJob001_02_Defenses",
    sCapturedLayer = "Vz_State_ChiJob001_02_Captured",
    sCapturedTgLayer = "Vz_State_ChiCon051c_Tg",
    sRivalFaction = "Vza",
    nStartingHealth = 6,
    nRusherQuota = 1
  }
end

function Activated(self)
  MrxTaskContractOutpost.Activated(self)
  MrxVoSequence.Start({
    "Fiona-In-Mission-Contract-Chi051-01",
    0.5,
    {
      mattias = "Mattias-In-Mission-Contract-Chi051-02",
      jennifer = "Jennifer-In-Mission-Contract-Chi051-03",
      chris = "Chris-In-Mission-Contract-Chi051-04"
    }
  })
end
