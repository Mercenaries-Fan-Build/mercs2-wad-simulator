inherit("MrxSupportDelivery")
inherit("MrxSupportDesignatorFlare")

function Create(oSelf, uOwnerGuid)
  local oNewSupport = MrxSupportDelivery:Create(uOwnerGuid)
  oNewSupport.Create = Create
  oNewSupport:SetCargo(oSelf.sCargoToDeliver)
  oNewSupport:SetModuleName("MrxBoatDelivery")
  local oDesignator = MrxSupportDesignatorFlare:Create()
  oDesignator:SetAATestLevel("none")
  oNewSupport:SetDesignator(oDesignator)
  return oNewSupport
end
