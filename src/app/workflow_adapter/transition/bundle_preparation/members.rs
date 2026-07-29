use super::super::*;

pub(crate) fn bind_additional_members(
    bundle: &mut PreparedSourceBundle,
    mut members: Vec<PreparedMember>,
) -> Result<(), AppError> {
    members.sort_by(prepared_member_order);
    let source_member_count = if bundle.source_install.is_some() {
        3
    } else {
        0
    };
    bundle.projection_lag_member_index = members
        .iter()
        .position(|member| member.kind == PreparedMemberKind::ProjectionLag)
        .map(|index| {
            u64::try_from(index + source_member_count)
                .map_err(|_| AppError::blocked("prepared projection lag index overflow"))
        })
        .transpose()?;
    bundle.additional_members = members;
    validate_prepared_source_bundle(bundle)
}
